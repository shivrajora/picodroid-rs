// SPDX-License-Identifier: GPL-3.0-only
package picodroid.pio;

/**
 * Opens the board's peripherals by name: {@code GP<n>} for a pad, {@code UART<n>}, {@code I2C<n>}
 * and {@code SPI<n>} for a bus.
 *
 * <p>A pad the board's own wiring owns (the display's control and bus lines, the touch
 * controller's, the hardware buttons, the buzzer) and the display's SPI bus are refused with an
 * {@link IllegalArgumentException} naming the line. The same pin number is spare on one board and
 * the display's DC line on another, so an app driving optional hardware catches it and carries on
 * without.
 */
public class PeripheralManager {
  private PeripheralManager() {}

  public static native PeripheralManager getInstance();

  public native Gpio openGpio(String name);

  public native UartDevice openUartDevice(String name);

  public native I2cDevice openI2cDevice(String name);

  public native SpiDevice openSpiDevice(String name);

  public native Pwm openPwm(String name);

  public native Adc openAdcPin(String name);
}
