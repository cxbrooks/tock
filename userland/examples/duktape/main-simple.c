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


static duk_ret_t native_print(duk_context *ctx) {
  duk_push_string(ctx, " ");
  duk_insert(ctx, 0);
  duk_join(ctx, duk_get_top(ctx) - 1);
#ifdef __ARM_EABI__
  putstr(duk_safe_to_string(ctx, -1));
#else
  printf("%s\n", duk_safe_to_string(ctx, -1));
#endif
  return 0;
}

static duk_ret_t native_adder(duk_context *ctx) {
  int i;
  int n = duk_get_top(ctx);  /* #args */
  double res = 0.0;

  for (i = 0; i < n; i++) {
    res += duk_to_number(ctx, i);
  }

  duk_push_number(ctx, res);
  return 1;  /* one return value */
}

int main(int argc, char *argv[]) {
  putstr("duktape/main-simple.c: start\n");
  duk_context *ctx = duk_create_heap_default();
  putstr("duktape/main-simple.c: after heap create\n");

  (void) argc; (void) argv;  /* suppress warning */

  putstr("duktape/main-simple.c: before pushing native_print 2\n");
  duk_push_c_function(ctx, native_print, DUK_VARARGS);
  putstr("duktape/main-simple.c: after pushing native_print\n");

  duk_put_global_string(ctx, "print");
  putstr("duktape/main-simple.c: after put_global print\n");

  duk_eval_string(ctx, "print('Hello world!');");
  putstr("duktape/main-simple.c: after eval print1\n");

  duk_push_c_function(ctx, native_adder, DUK_VARARGS);
  putstr("duktape/main-simple.c: after pushing native_adder\n");

  duk_put_global_string(ctx, "adder");
  putstr("duktape/main-simple.c: after put_global print\n");


  duk_eval_string(ctx, "print('2+3=' + adder(2, 3));");
  putstr("duktape/main-simple.c: after eval print2\n");

  duk_pop(ctx);  /* pop eval result */

  duk_destroy_heap(ctx);
  putstr("duktape/main-simple.c: done\n");
  return 0;
}
