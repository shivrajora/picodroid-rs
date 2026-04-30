package picoenvmon.hardware;

import picodroid.pio.PeripheralManager;
import picodroid.pio.Pwm;
import picodroid.util.Log;

/**
 * Pimoroni Enviro+ Pack RGB LED (common-anode active-low) on R=GP6, G=GP7, B=GP10. Pre-allocated at
 * the app scope (one LED on the board) and driven via PWM at ~1 kHz.
 */
public class RgbLed {
  private static final String TAG = "RgbLed";
  private static final double PWM_FREQ_HZ = 1000.0;

  private final Pwm red;
  private final Pwm green;
  private final Pwm blue;

  public RgbLed() {
    PeripheralManager pm = PeripheralManager.getInstance();
    this.red = pm.openPwm("GP6");
    this.green = pm.openPwm("GP7");
    this.blue = pm.openPwm("GP10");
    setupChannel(red);
    setupChannel(green);
    setupChannel(blue);
    Log.i(TAG, "RGB LED ready on GP6/GP7/GP10");
    setColor(0, 0, 0);
  }

  private static void setupChannel(Pwm ch) {
    ch.setPwmFrequencyHz(PWM_FREQ_HZ);
    ch.setEnabled(true);
  }

  /** Each component in {@code [0..255]}. Common-anode → 0% duty = full on. */
  public void setColor(int r, int g, int b) {
    red.setPwmDutyCycle(toDuty(r));
    green.setPwmDutyCycle(toDuty(g));
    blue.setPwmDutyCycle(toDuty(b));
  }

  public void off() {
    setColor(0, 0, 0);
  }

  private static double toDuty(int v) {
    int clamped = v < 0 ? 0 : (v > 255 ? 255 : v);
    return 100.0 * (1.0 - clamped / 255.0);
  }
}
