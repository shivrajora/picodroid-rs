package blinky;

import picodroid.os.SystemClock;
import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

public class LedBlink {
  public static void main(String[] args) {
    Log.i("HelloWorld", "Hello, World!");

    PeripheralManager manager = PeripheralManager.getInstance();
    Gpio led = manager.openGpio("GP25");
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
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
