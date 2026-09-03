// SPDX-License-Identifier: GPL-3.0-only
package blinky;

import picodroid.app.Application;
import picodroid.os.SystemClock;
import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

public class LedBlink extends Application {
  @Override
  public void onCreate() {
    Log.i("HelloWorld", "Hello, World!");

    PeripheralManager manager = PeripheralManager.getInstance();
    Gpio led = manager.openGpio("GP25");
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    // Input read-back: GP16 is unconnected on the testbench, so the level is
    // whatever the pad floats to (the sim reads LOW).
    Gpio sense = manager.openGpio("GP16");
    sense.setDirection(Gpio.DIRECTION_IN);
    Log.i("GPIO", "GP16 reads " + (sense.getValue() ? "HIGH" : "LOW"));
    while (true) {
      led.setValue(true);
      Log.i("LED", "on");
      SystemClock.sleep(500);
      led.setValue(false);
      Log.i("LED", "off");
      SystemClock.sleep(500);
    }
  }
}
