#include "wectr.h"

#include "wasm_export.h"

#include <stdlib.h>
#include <string.h>

#define WECTR_DEFAULT_STACK (64 * 1024)
#define WECTR_DEFAULT_HEAP (64 * 1024)

struct wectr {
  wasm_module_t module;
  wasm_module_inst_t inst;
  wasm_exec_env_t env;

  wasm_function_inst_t fn_start;
  wasm_function_inst_t fn_tick;
  wasm_function_inst_t fn_dispatch;

  wectr_on_emit on_emit;
  void *user;

  uint64_t subscriptions;

  uint8_t *wasm;
  char error[128];
};

static wectr *self(wasm_exec_env_t env) {
  return (wectr *) wasm_runtime_get_user_data(env);
}

static int64_t native_emit(wasm_exec_env_t env, uint32_t channel, void *buf,
                           uint32_t len) {
  wectr *w = self(env);
  if (!w || !w->on_emit)
    return 0;
  return w->on_emit(w->user, channel, buf, len);
}

static void native_subscribe(wasm_exec_env_t env, uint32_t channel) {
  wectr *w = self(env);
  if (w && channel < 64)
    w->subscriptions |= (uint64_t) 1 << channel;
}

static void native_unsubscribe(wasm_exec_env_t env, uint32_t channel) {
  wectr *w = self(env);
  if (w && channel < 64)
    w->subscriptions &= ~((uint64_t) 1 << channel);
}

static NativeSymbol wectr_natives[] = {
    {"wectr_emit", (void *) native_emit, "(i*~)I", NULL},
    {"wectr_subscribe", (void *) native_subscribe, "(i)", NULL},
    {"wectr_unsubscribe", (void *) native_unsubscribe, "(i)", NULL},
};

wectr_status wectr_global_init(void) {
  if (!wasm_runtime_init())
    return WECTR_ERR_INSTANTIATE;
  uint32_t n = sizeof(wectr_natives) / sizeof(wectr_natives[0]);
  if (!wasm_runtime_register_natives("wectr", wectr_natives, n)) {
    wasm_runtime_destroy();
    return WECTR_ERR_INSTANTIATE;
  }
  return WECTR_OK;
}

void wectr_global_destroy(void) {
  wasm_runtime_destroy();
}

static void set_error(wectr *w, const char *msg) {
  if (!msg)
    msg = "";
  strncpy(w->error, msg, sizeof(w->error) - 1);
  w->error[sizeof(w->error) - 1] = '\0';
}

static void capture_trap(wectr *w) {
  const char *ex = wasm_runtime_get_exception(w->inst);
  set_error(w, ex);
  wasm_runtime_clear_exception(w->inst);
}

wectr *wectr_open(const uint8_t *wasm, size_t wasm_len, const wectr_config *cfg,
                  wectr_status *status) {
  wectr_status ignored;
  if (!status)
    status = &ignored;

  if (!wasm || !wasm_len || !cfg || !cfg->on_emit) {
    *status = WECTR_ERR_ARG;
    return NULL;
  }

  wectr *w = calloc(1, sizeof(*w));
  if (!w) {
    *status = WECTR_ERR_OOM;
    return NULL;
  }
  w->on_emit = cfg->on_emit;
  w->user = cfg->user;

  w->wasm = malloc(wasm_len);
  if (!w->wasm) {
    free(w);
    *status = WECTR_ERR_OOM;
    return NULL;
  }
  memcpy(w->wasm, wasm, wasm_len);

  char err[128];
  w->module = wasm_runtime_load(w->wasm, (uint32_t) wasm_len, err, sizeof(err));
  if (!w->module) {
    set_error(w, err);
    *status = WECTR_ERR_LOAD;
    goto fail_after_wasm;
  }

  uint32_t stack = cfg->stack_size ? cfg->stack_size : WECTR_DEFAULT_STACK;
  uint32_t heap = cfg->heap_size ? cfg->heap_size : WECTR_DEFAULT_HEAP;
  w->inst = wasm_runtime_instantiate(w->module, stack, heap, err, sizeof(err));
  if (!w->inst) {
    set_error(w, err);
    *status = WECTR_ERR_INSTANTIATE;
    goto fail_after_load;
  }

  w->env = wasm_runtime_create_exec_env(w->inst, stack);
  if (!w->env) {
    set_error(w, "could not create exec env");
    *status = WECTR_ERR_INSTANTIATE;
    goto fail_after_inst;
  }
  wasm_runtime_set_user_data(w->env, w);

  w->fn_start = wasm_runtime_lookup_function(w->inst, "wectr_start");
  w->fn_tick = wasm_runtime_lookup_function(w->inst, "wectr_tick");
  w->fn_dispatch = wasm_runtime_lookup_function(w->inst, "wectr_dispatch");
  if (!w->fn_start || !w->fn_tick) {
    set_error(w, "guest missing wectr_start / wectr_tick export");
    *status = WECTR_ERR_NO_ENTRY;
    goto fail_after_env;
  }

  *status = WECTR_OK;
  return w;

fail_after_env:
  wasm_runtime_destroy_exec_env(w->env);
fail_after_inst:
  wasm_runtime_deinstantiate(w->inst);
fail_after_load:
  wasm_runtime_unload(w->module);
fail_after_wasm:
  free(w->wasm);
  free(w);
  return NULL;
}

void wectr_close(wectr *w) {
  if (!w)
    return;
  if (w->env)
    wasm_runtime_destroy_exec_env(w->env);
  if (w->inst)
    wasm_runtime_deinstantiate(w->inst);
  if (w->module)
    wasm_runtime_unload(w->module);
  free(w->wasm);
  free(w);
}

wectr_status wectr_start(wectr *w) {
  if (!w)
    return WECTR_ERR_ARG;
  if (!wasm_runtime_call_wasm(w->env, w->fn_start, 0, NULL)) {
    capture_trap(w);
    return WECTR_ERR_TRAP;
  }
  return WECTR_OK;
}

int wectr_tick(wectr *w, int64_t elapsed_ms) {
  if (!w)
    return -1;

  wasm_val_t args[1] = {{.kind = WASM_I64, .of.i64 = elapsed_ms}};
  wasm_val_t results[1] = {{.kind = WASM_I32, .of.i32 = 0}};
  if (!wasm_runtime_call_wasm_a(w->env, w->fn_tick, 1, results, 1, args)) {
    capture_trap(w);
    return -1;
  }
  return results[0].of.i32;
}

int wectr_is_subscribed(const wectr *w, uint32_t channel) {
  if (!w || channel >= 64)
    return 0;
  return (w->subscriptions & ((uint64_t) 1 << channel)) != 0;
}

wectr_status wectr_dispatch(wectr *w, uint32_t channel, const void *data,
                            uint32_t len) {
  if (!w)
    return WECTR_ERR_ARG;
  if (!w->fn_dispatch)
    return WECTR_ERR_NO_ENTRY;
  if (!wectr_is_subscribed(w, channel))
    return WECTR_OK;

  uint64_t app_ptr = 0;
  void *native = NULL;
  if (len) {
    app_ptr = wasm_runtime_module_malloc(w->inst, len, &native);
    if (!app_ptr)
      return WECTR_ERR_OOM;
    memcpy(native, data, len);
  }

  uint32_t argv[3] = {channel, (uint32_t) app_ptr, len};
  wectr_status st = WECTR_OK;
  if (!wasm_runtime_call_wasm(w->env, w->fn_dispatch, 3, argv)) {
    capture_trap(w);
    st = WECTR_ERR_TRAP;
  }

  if (app_ptr)
    wasm_runtime_module_free(w->inst, app_ptr);
  return st;
}

const char *wectr_last_error(const wectr *w) {
  if (!w || w->error[0] == '\0')
    return NULL;
  return w->error;
}
