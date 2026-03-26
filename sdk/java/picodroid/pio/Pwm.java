package picodroid.pio;

public class Pwm {
  private int pin;

  // Package-private — created via PeripheralManager.openPwm()
  Pwm(int pin) {
    this.pin = pin;
  }

  /**
   * Enables or disables the PWM output. The frequency must be set before enabling. When disabled,
   * the output pin holds its last state.
   */
  public native void setEnabled(boolean enabled);

  /** Sets the PWM duty cycle as a percentage (0.0–100.0 inclusive). */
  public native void setPwmDutyCycle(double dutyCycle);

  /** Sets the PWM output frequency in Hertz. Must be called before setEnabled(true). */
  public native void setPwmFrequencyHz(double freqHz);

  public native void close();
}
