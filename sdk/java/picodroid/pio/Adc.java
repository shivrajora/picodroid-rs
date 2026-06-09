// SPDX-License-Identifier: GPL-3.0-only
package picodroid.pio;

public class Adc implements AutoCloseable {
  private int pin;

  Adc(int pin) {
    this.pin = pin;
  }

  public native double readValue();

  @Override
  public native void close();
}
