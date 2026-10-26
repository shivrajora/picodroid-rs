// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import picodroid.media.AudioManager;
import picodroid.media.ToneGenerator;
import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

/**
 * The ring: the board's buzzer through {@link ToneGenerator}, with the EP-0172 carrier's red LED on
 * GP16 flashing in step so an alarm is visible as well as audible.
 *
 * <p>The buzzer is the board's own sound output (board.toml's {@code [audio]} section), not a pin
 * for the app to open: {@code PeripheralManager} refuses it like any pad the board's wiring owns.
 * On a board with no {@code [audio]} section the generator is silent and {@code startTone} answers
 * false, so the alarm still rings on screen. The LED is optional hardware on the same terms.
 */
public final class Ringer {
  private static final String TAG = ClockApp.TAG;

  /** Carrier LED, flashed with the beat. */
  private static final String LED_PIN = "GP16";

  /**
   * One cycle of the ring, eight beats of {@link AlarmService#BEAT_MS}: three beats of a low note
   * and a beat of rest, then the same on a high note — the two-note pattern a clock radio wakes you
   * with. Both notes sit near the piezo's 2.4 kHz resonance, where it is loudest; {@code
   * startToneSequence} is the one way to name a frequency, since every Android tone is a fixed
   * telephony one.
   */
  private static final int[] CYCLE_HZ = {1760, 0, 2349, 0};

  private static final int[] CYCLE_MS = {750, 250, 750, 250};

  /** Beats in one {@link #CYCLE_HZ} cycle. */
  private static final int BEATS_PER_CYCLE = 8;

  /**
   * How loud: {@code ToneGenerator} maps 0-100 onto a 0-50 % duty cycle, and a fifth of the range
   * (10 % duty) on this carrier's piezo is audible across a room without being startling.
   */
  private static final int VOLUME = 20;

  private ToneGenerator tones;
  private Gpio led;
  private boolean sounding;

  public Ringer() {
    tones = new ToneGenerator(AudioManager.STREAM_ALARM, VOLUME);
    try {
      led = PeripheralManager.getInstance().openGpio(LED_PIN);
      led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    } catch (RuntimeException e) {
      led = null;
      Log.w(TAG, "no LED on " + LED_PIN + ": " + e);
    }
  }

  /**
   * Beat {@code n} of the ring, one call per beat while an alarm is up. The tone restarts at the
   * top of each cycle (a sequence plays once); the LED is lit for the notes and dark for the rests.
   */
  public void beat(int n) {
    if (n % BEATS_PER_CYCLE == 0 && tones != null) {
      boolean started = tones.startToneSequence(CYCLE_HZ, CYCLE_MS);
      if (n == 0) {
        Log.i(TAG, "ring tone " + (started ? "started" : "unavailable"));
      }
    }
    sounding = true;
    if (led != null) {
      led.setValue(n % 4 != 3);
    }
  }

  /** Silence the tone and darken the LED. Safe to call when already silent. */
  public void off() {
    if (!sounding) {
      return;
    }
    sounding = false;
    if (tones != null) {
      tones.stopTone();
    }
    if (led != null) {
      led.setValue(false);
    }
    Log.i(TAG, "ring tone stopped");
  }

  /** Release the generator and the LED — the service's {@code onDestroy}. */
  public void release() {
    off();
    if (tones != null) {
      tones.release();
      tones = null;
    }
    if (led != null) {
      led.close();
      led = null;
    }
  }
}
