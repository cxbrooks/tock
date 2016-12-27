#include <stdio.h>

#include <led.h>
#include <FXOS8700CQ.h>

int main() {
  int x, y, z;

  // Choose the LED to use. We want green (which is usually
  // second in RGB), but will take anything.
  int led = 0;
  int num_leds = led_count();
  if (num_leds > 1) led = 1;

  while(1) {
    FXOS8700CQ_read_magenetometer_sync(&x, &y, &z);
    printf("x: %d, y: %d, z: %d\n", x, y, z);

    int absx=x, absy=y, absz=z;
    if (x < 0) absx = x*-1;
    if (y < 0) absy = y*-1;
    if (z < 0) absz = z*-1;

    if (x < 0 && (absx > 2*absy) && (absx > 2*absz)) {
      led_on(led);
    } else {
      led_off(led);
    }
  }

  return 0;
}