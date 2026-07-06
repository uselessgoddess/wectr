#include "wectr.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { IN_AMMO = 0 };
enum { OUT_MOVE = 0, OUT_HIDE = 1 };

typedef struct {
  uint32_t rounds;
} Ammo;
typedef struct {
  int32_t dx, dy;
} Move;

struct world {
  int x, y;
  int ammo;
};

static int64_t on_emit(void *user, uint32_t channel, const void *data,
                       uint32_t len) {
  struct world *w = user;
  switch (channel) {
    case OUT_MOVE: {
      Move m = {0, 0};
      memcpy(&m, data, len < sizeof m ? len : sizeof m);
      w->x += m.dx;
      w->y += m.dy;
      printf("  move -> (%d, %d)\n", w->x, w->y);
      break;
    }
    case OUT_HIDE:
      printf("  out of ammo\n");
      break;
  }
  return 0;
}

static uint8_t *read_file(const char *path, size_t *len) {
  FILE *f = fopen(path, "rb");
  if (!f) {
    perror(path);
    return NULL;
  }
  fseek(f, 0, SEEK_END);
  long n = ftell(f);
  fseek(f, 0, SEEK_SET);
  uint8_t *buf = malloc(n > 0 ? (size_t) n : 1);
  if (!buf || fread(buf, 1, (size_t) n, f) != (size_t) n) {
    fprintf(stderr, "could not read %s\n", path);
    free(buf);
    fclose(f);
    return NULL;
  }
  fclose(f);
  *len = (size_t) n;
  return buf;
}

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: %s <bot.wasm>\n", argv[0]);
    return 2;
  }

  size_t wasm_len = 0;
  uint8_t *wasm = read_file(argv[1], &wasm_len);
  if (!wasm)
    return 1;

  if (wectr_global_init() != WECTR_OK) {
    fprintf(stderr, "runtime init failed\n");
    free(wasm);
    return 1;
  }

  struct world world = {.x = 0, .y = 0, .ammo = 3};
  wectr_config cfg = {.on_emit = on_emit, .user = &world};
  wectr_status st;
  wectr *w = wectr_open(wasm, wasm_len, &cfg, &st);
  free(wasm);
  if (!w) {
    fprintf(stderr, "open failed (%d)\n", st);
    wectr_global_destroy();
    return 1;
  }

  int rc = 0;
  if (wectr_start(w) != WECTR_OK) {
    fprintf(stderr, "start trapped: %s\n", wectr_last_error(w));
    rc = 1;
    goto done;
  }

  for (int frame = 0; frame < 1000; frame++) {
    if (wectr_is_subscribed(w, IN_AMMO)) {
      Ammo a = {(uint32_t) world.ammo};
      wectr_dispatch(w, IN_AMMO, &a, sizeof a);
      if (world.ammo > 0)
        world.ammo--;
    }

    int running = wectr_tick(w, 100);
    if (running < 0) {
      fprintf(stderr, "tick trapped: %s\n", wectr_last_error(w));
      rc = 1;
      goto done;
    }
    if (!running) {
      printf("finished at (%d, %d)\n", world.x, world.y);
      goto done;
    }
  }
  fprintf(stderr, "script did not finish\n");
  rc = 1;

done:
  wectr_close(w);
  wectr_global_destroy();
  return rc;
}
