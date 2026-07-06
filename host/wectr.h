#ifndef WECTR_H
#define WECTR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct wectr wectr;

typedef enum {
  WECTR_OK = 0,
  WECTR_ERR_ARG = -1,
  WECTR_ERR_LOAD = -2,
  WECTR_ERR_INSTANTIATE = -3,
  WECTR_ERR_NO_ENTRY = -4,
  WECTR_ERR_TRAP = -5,
  WECTR_ERR_OOM = -6,
} wectr_status;

typedef int64_t (*wectr_on_emit)(void *user, uint32_t channel, const void *data,
                                 uint32_t len);

typedef struct {
  wectr_on_emit on_emit;
  void *user;
  uint32_t stack_size;
  uint32_t heap_size;
} wectr_config;

wectr_status wectr_global_init(void);
void wectr_global_destroy(void);

wectr *wectr_open(const uint8_t *wasm, size_t wasm_len, const wectr_config *cfg,
                  wectr_status *status);

wectr_status wectr_start(wectr *w);

int wectr_tick(wectr *w, int64_t elapsed_ms);

wectr_status wectr_dispatch(wectr *w, uint32_t channel, const void *data,
                            uint32_t len);

int wectr_is_subscribed(const wectr *w, uint32_t channel);

const char *wectr_last_error(const wectr *w);

void wectr_close(wectr *w);

#ifdef __cplusplus
}
#endif

#endif /* WECTR_H */
