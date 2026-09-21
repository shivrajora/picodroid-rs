// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.hardware;

import picodroid.pio.PeripheralManager;
import picodroid.pio.Pwm;
import picodroid.util.Log;

/**
 * The Display Pack 2.0's RGB LED: common-anode on GP6 / GP7 / GP8, so PWM duty is inverted (100 %
 * is dark). GP6 and GP7 share PWM slice 3, GP8 is on slice 4; one frequency suits all three.
 *
 * <p>Optional hardware: on a board without these pins {@link #open} returns a dummy.
 */
public final class RgbLed {
  private static final String TAG = "RgbLed";
  private static final double PWM_FREQ_HZ = 1000.0;

  private final Pwm red;
  private final Pwm green;
  private final Pwm blue;
  private int shown = -1;

  private RgbLed(Pwm red, Pwm green, Pwm blue) {
    this.red = red;
    this.green = green;
    this.blue = blue;
  }

  public static RgbLed open() {
    try {
      PeripheralManager pm = PeripheralManager.getInstance();
      RgbLed led = new RgbLed(channel(pm, "GP6"), channel(pm, "GP7"), channel(pm, "GP8"));
      led.setColor(0);
      return led;
    } catch (RuntimeException e) {
      Log.i(TAG, "no RGB LED: " + e);
      return new RgbLed(null, null, null);
    }
  }

  private static Pwm channel(PeripheralManager pm, String pin) {
    Pwm ch = pm.openPwm(pin);
    ch.setPwmFrequencyHz(PWM_FREQ_HZ);
    ch.setEnabled(true);
    return ch;
  }

  /** 0xRRGGBB; 0 is off. The LED is bright at arm's length, so callers pass small values. */
  public void setColor(int rgb) {
    if (red == null || rgb == shown) {
      return;
    }
    shown = rgb;
    red.setPwmDutyCycle(duty((rgb >> 16) & 0xFF));
    green.setPwmDutyCycle(duty((rgb >> 8) & 0xFF));
    blue.setPwmDutyCycle(duty(rgb & 0xFF));
  }

  private static double duty(int v) {
    return 100.0 * (1.0 - v / 255.0);
  }
}
