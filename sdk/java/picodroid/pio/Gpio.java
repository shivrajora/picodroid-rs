// SPDX-License-Identifier: GPL-3.0-only
package picodroid.pio;

public class Gpio implements AutoCloseable {
  /** Input, no pull (Android Things value). Read it with {@link #getValue()}. */
  public static final int DIRECTION_IN = 0;

  public static final int DIRECTION_OUT_INITIALLY_HIGH = 1;
  public static final int DIRECTION_OUT_INITIALLY_LOW = 2;

  private int pin;

  // Package-private — created via PeripheralManager.openGpio()
  Gpio(int pin) {
    this.pin = pin;
  }

  public native void setDirection(int direction);

  public native void setValue(boolean value);

  /** Current pin level. On an output this is the driven level. */
  public native boolean getValue();

  @Override
  public native void close();
}
