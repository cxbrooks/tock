/*
 *  Very simple example program
 */

#include "duktape.h"

#ifdef __ARM_EABI__
#include "console.h"
int _gettimeofday(struct timeval *tp, void *tzp) {
  return 0;
}
#else
#define putstr(x) fprintf(stderr, "%s", x);
#endif

void ledcontrol_register(duk_context *ctx);

int main(int argc, char *argv[]) {
  (void) argc; (void) argv;  /* suppress warning */

  putstr("duktape/main-led.c: start\n");
  duk_context *ctx = duk_create_heap_default();

  putstr("duktape/main-led.c: ledcontrol_register()\n");
  ledcontrol_register(ctx);
  
  duk_eval_string(ctx, "LedControl.toggle(2);");

  duk_pop(ctx);  /* pop eval result */

  duk_destroy_heap(ctx);
  putstr("duktape/main.c: done\n");
  return 0;
}
