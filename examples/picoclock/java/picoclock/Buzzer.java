// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.pio.Pwm;
import picodroid.util.Log;

/**
 * The ring: the EP-0172 carrier's buzzer on GP13, with its red LED on GP16 flashing in step so an
 * alarm is visible as well as audible. Neither pin is claimed by the framework — board.toml leaves
 * GP12, GP13, GP16 and GP17 to apps — so this opens both directly.
 *
 * <p>Opening is best effort. On a board without these pins, or with them already held, the alarm
 * still rings on screen and this falls silent rather than taking the app down with it.
 */
public final class Buzzer {
  private static final String TAG = ClockApp.TAG;

  /** Carrier buzzer. A passive sounder, so it needs a tone, not a level. */
  private static final String BUZZER_PIN = "GP13";

  /** Carrier LED, flashed with the beat. */
  private static final String LED_PIN = "GP16";

  /** Two alternating tones, the two-note pattern a clock radio wakes you with. */
  private static final double LOW_HZ = 1760.0;

  private static final double HIGH_HZ = 2349.0;

  /**
   * How loud the sounder is. Half duty is the loudest a square-wave sounder gets, and on this
   * carrier's piezo that is far louder than a bedside alarm needs to be; a tenth is audible across
   * a room without being startling. Raise it towards 50 for a noisier one.
   */
  private static final double DUTY_PERCENT = 10.0;

  private Pwm pwm;
  private Gpio led;
  private boolean beeping;

  public Buzzer() {
    PeripheralManager pm = PeripheralManager.getInstance();
    try {
      pwm = pm.openPwm(BUZZER_PIN);
      pwm.setPwmFrequencyHz(LOW_HZ);
      pwm.setPwmDutyCycle(DUTY_PERCENT);
    } catch (RuntimeException e) {
      pwm = null;
      Log.w(TAG, "no buzzer on " + BUZZER_PIN + ": " + e);
    }
    try {
      led = pm.openGpio(LED_PIN);
      led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    } catch (RuntimeException e) {
      led = null;
      Log.w(TAG, "no LED on " + LED_PIN + ": " + e);
    }
  }

  /**
   * Sound beat {@code n} of the ring, one call per service tick while an alarm is up. The pattern
   * is two beeps and a rest over four ticks, the tone alternating between pairs.
   */
  public void beat(int n) {
    int phase = n % 4;
    if (phase == 3) {
      off();
      return;
    }
    on(n % 8 < 4 ? LOW_HZ : HIGH_HZ);
  }

  /** Silence the sounder and darken the LED. Safe to call when already silent. */
  public void off() {
    if (!beeping) {
      return;
    }
    beeping = false;
    if (pwm != null) {
      pwm.setEnabled(false);
    }
    if (led != null) {
      led.setValue(false);
    }
  }

  /** Release both pins — the service's {@code onDestroy}. */
  public void close() {
    off();
    if (pwm != null) {
      pwm.close();
      pwm = null;
    }
    if (led != null) {
      led.close();
      led = null;
    }
  }

  private void on(double hz) {
    if (pwm != null) {
      pwm.setPwmFrequencyHz(hz);
      pwm.setEnabled(true);
    }
    if (led != null) {
      led.setValue(true);
    }
    beeping = true;
  }
}
