// SPDX-License-Identifier: GPL-3.0-only
package pwmdemo;

import picodroid.app.Application;
import picodroid.os.SystemClock;
import picodroid.pio.PeripheralManager;
import picodroid.pio.Pwm;
import picodroid.util.Log;

/**
 * PWM LED fade demo.
 *
 * <p>Opens PWM on GP25 (onboard LED, slice 4 channel B) at 1 kHz and fades the brightness from 0%
 * to 100% and back, three times. On hardware, the onboard LED will visibly breathe. In simulation,
 * the duty cycle changes are logged to the console.
 */
public class PwmDemo extends Application {
  public void onCreate() {
    PeripheralManager pm = PeripheralManager.getInstance();
    Pwm pwm = pm.openPwm("GP25");

    pwm.setPwmFrequencyHz(1000.0);
    Log.i("PWM", "starting LED fade");

    for (int cycle = 0; cycle < 3; cycle++) {
      // Fade up: 0% → 100%
      for (int d = 0; d <= 100; d += 5) {
        pwm.setPwmDutyCycle(d);
        pwm.setEnabled(true);
        SystemClock.sleep(50);
      }
      // Fade down: 100% → 0%
      for (int d = 100; d >= 0; d -= 5) {
        pwm.setPwmDutyCycle(d);
        SystemClock.sleep(50);
      }
    }

    pwm.setEnabled(false);
    pwm.close();
    Log.i("PWM", "done");
  }
}
