// SPDX-License-Identifier: GPL-3.0-only
package picodroid.media;

/**
 * Plays the standard supervisory, proprietary and DTMF tones, mirroring {@code
 * android.media.ToneGenerator}. Constant names and values match Android's, so tone selection code
 * carries over unchanged.
 *
 * <p><b>What the hardware is.</b> On picodroid the output is a single piezo buzzer driven by one
 * PWM channel, so every tone is a square wave and only one frequency sounds at a time. Android's
 * tone table is largely multi-frequency: {@link #TONE_PROP_BEEP} is 400 Hz plus 1200 Hz, and each
 * DTMF digit is a row/column pair. Here the <em>lowest</em> frequency of each tone is what plays.
 * Tones stay recognisable, but they are not spectrally correct, and in particular <b>the DTMF tones
 * will not decode</b> on a receiver: dropping the column frequency collapses the sixteen digits
 * onto the four row frequencies, so {@link #TONE_DTMF_1}, {@link #TONE_DTMF_2}, {@link
 * #TONE_DTMF_3} and {@link #TONE_DTMF_A} all sound identical. They are provided so that ported code
 * compiles and beeps, not as a signalling facility.
 *
 * <p>Cadences follow Android's CEPT variant. Tones with no stated duration play until {@link
 * #stopTone()}; tones with a repeat cadence repeat until stopped. There is one buzzer, so one tone
 * sounds at a time across the whole system: starting a tone replaces whatever was playing, and the
 * volume of the most recently constructed generator is the one in effect.
 *
 * <p>The CDMA tone constants and {@code TONE_SUP_INTERCEPT} are not implemented; {@link
 * #startTone(int)} returns {@code false} for any tone this class does not define.
 *
 * <pre>{@code
 * ToneGenerator tg = new ToneGenerator(AudioManager.STREAM_SYSTEM, 80);
 * tg.startTone(ToneGenerator.TONE_PROP_BEEP);
 * ...
 * tg.release();
 * }</pre>
 */
public class ToneGenerator {
  /** DTMF 0: 941 Hz + 1336 Hz. Plays 941 Hz. */
  public static final int TONE_DTMF_0 = 0;

  /** DTMF 1: 697 Hz + 1209 Hz. Plays 697 Hz. */
  public static final int TONE_DTMF_1 = 1;

  /** DTMF 2: 697 Hz + 1336 Hz. Plays 697 Hz. */
  public static final int TONE_DTMF_2 = 2;

  /** DTMF 3: 697 Hz + 1477 Hz. Plays 697 Hz. */
  public static final int TONE_DTMF_3 = 3;

  /** DTMF 4: 770 Hz + 1209 Hz. Plays 770 Hz. */
  public static final int TONE_DTMF_4 = 4;

  /** DTMF 5: 770 Hz + 1336 Hz. Plays 770 Hz. */
  public static final int TONE_DTMF_5 = 5;

  /** DTMF 6: 770 Hz + 1477 Hz. Plays 770 Hz. */
  public static final int TONE_DTMF_6 = 6;

  /** DTMF 7: 852 Hz + 1209 Hz. Plays 852 Hz. */
  public static final int TONE_DTMF_7 = 7;

  /** DTMF 8: 852 Hz + 1336 Hz. Plays 852 Hz. */
  public static final int TONE_DTMF_8 = 8;

  /** DTMF 9: 852 Hz + 1477 Hz. Plays 852 Hz. */
  public static final int TONE_DTMF_9 = 9;

  /** DTMF *: 941 Hz + 1209 Hz. Plays 941 Hz. */
  public static final int TONE_DTMF_S = 10;

  /** DTMF #: 941 Hz + 1477 Hz. Plays 941 Hz. */
  public static final int TONE_DTMF_P = 11;

  /** DTMF A: 697 Hz + 1633 Hz. Plays 697 Hz. */
  public static final int TONE_DTMF_A = 12;

  /** DTMF B: 770 Hz + 1633 Hz. Plays 770 Hz. */
  public static final int TONE_DTMF_B = 13;

  /** DTMF C: 852 Hz + 1633 Hz. Plays 852 Hz. */
  public static final int TONE_DTMF_C = 14;

  /** DTMF D: 941 Hz + 1633 Hz. Plays 941 Hz. */
  public static final int TONE_DTMF_D = 15;

  /** Dial tone: 425 Hz, continuous until {@link #stopTone()}. */
  public static final int TONE_SUP_DIAL = 16;

  /** Busy: 425 Hz, 500 ms on, 500 ms off, repeating. */
  public static final int TONE_SUP_BUSY = 17;

  /** Congestion: 425 Hz, 200 ms on, 200 ms off, repeating. */
  public static final int TONE_SUP_CONGESTION = 18;

  /** Radio acknowledge: 425 Hz, 200 ms on. */
  public static final int TONE_SUP_RADIO_ACK = 19;

  /** Radio not available: 425 Hz, 200 ms on, 200 ms off, 3 bursts. */
  public static final int TONE_SUP_RADIO_NOTAVAIL = 20;

  /** Error: 950 Hz + 1400 Hz + 1800 Hz, 330 ms on, 1 s off, repeating. Plays 950 Hz. */
  public static final int TONE_SUP_ERROR = 21;

  /** Call waiting: 425 Hz, 200 ms on, 600 ms off, 200 ms on, 3 s off, repeating. */
  public static final int TONE_SUP_CALL_WAITING = 22;

  /** Ringtone: 425 Hz, 1 s on, 4 s off, repeating. */
  public static final int TONE_SUP_RINGTONE = 23;

  /** Beep: 400 Hz + 1200 Hz, 35 ms on. Plays 400 Hz. */
  public static final int TONE_PROP_BEEP = 24;

  /** Positive acknowledgement: 1200 Hz, 100 ms on, 100 ms off, 2 bursts. */
  public static final int TONE_PROP_ACK = 25;

  /** Negative acknowledgement: 300 Hz + 400 Hz + 500 Hz, 400 ms on. Plays 300 Hz. */
  public static final int TONE_PROP_NACK = 26;

  /** Prompt: 400 Hz + 1200 Hz, 200 ms on. Plays 400 Hz. */
  public static final int TONE_PROP_PROMPT = 27;

  /** Double beep: 400 Hz + 1200 Hz, 35 ms on, 200 ms off, 2 bursts. Plays 400 Hz. */
  public static final int TONE_PROP_BEEP2 = 28;

  /** Confirm: 350 Hz + 440 Hz, 100 ms on, 100 ms off, 3 bursts. Plays 350 Hz. */
  public static final int TONE_SUP_CONFIRM = 32;

  /** Pip: 480 Hz, 100 ms on, 100 ms off, 4 bursts. */
  public static final int TONE_SUP_PIP = 33;

  /** Largest number of segments {@link #startToneSequence} accepts. */
  public static final int MAX_SEQUENCE_LENGTH = 32;

  /**
   * Creates a generator. {@code streamType} is accepted for source compatibility with Android and
   * has no effect: there is one buzzer and no per-stream mixer or volume policy behind it. Use one
   * of the {@code STREAM_} constants on {@link AudioManager}.
   *
   * @param streamType the stream the tones would belong to on Android; ignored here
   * @param volume 0 to 100. A square wave is loudest at a 50% duty cycle, so this scales the duty
   *     cycle from 0% at {@code 0} to 50% at {@code 100}. Values outside the range are clamped.
   */
  public ToneGenerator(int streamType, int volume) {
    nativeInit(streamType, volume);
  }

  /**
   * Starts a tone, playing until it completes its cadence or {@link #stopTone()} is called.
   *
   * @param toneType one of the {@code TONE_} constants
   * @return true if the tone started, false if this class does not define {@code toneType}
   */
  public boolean startTone(int toneType) {
    return startTone(toneType, 0);
  }

  /**
   * Starts a tone, stopping it after {@code durationMs} whatever its cadence would otherwise do.
   *
   * @param toneType one of the {@code TONE_} constants
   * @param durationMs how long to play for, or 0 to let the tone run its natural course
   * @return true if the tone started, false if this class does not define {@code toneType}
   */
  public native boolean startTone(int toneType, int durationMs);

  /**
   * Plays an arbitrary sequence of notes.
   *
   * <p><b>This method has no {@code android.media} counterpart.</b> Android has no API for an
   * app-supplied note sequence, because on a phone a melody is an audio file played through {@code
   * MediaPlayer} or {@code SoundPool}, and neither can work on a buzzer. It is a picodroid
   * extension, and code using it will not compile against Android.
   *
   * <p>The two arrays are read in step: entry <i>i</i> plays {@code freqHz[i]} for {@code
   * durationMs[i]} milliseconds. A frequency of 0 is a rest. The note data is copied out before
   * this returns, so the caller may reuse or discard the arrays immediately. The sequence plays
   * once and does not repeat.
   *
   * @param freqHz frequency of each note in Hz, or 0 for a rest
   * @param durationMs duration of each note in milliseconds
   * @return true if the sequence started; false if either array is null, they differ in length, or
   *     the sequence is empty or longer than {@link #MAX_SEQUENCE_LENGTH}
   */
  public native boolean startToneSequence(int[] freqHz, int[] durationMs);

  /** Silences whatever is playing. Does nothing if no tone is sounding. */
  public native void stopTone();

  /**
   * Stops any tone and releases the buzzer, leaving the pin idle rather than holding a level.
   *
   * <p>Android leaves the state of a released generator undefined. Here, because a single buzzer is
   * shared by the whole system rather than owned per instance, a released generator simply works
   * again if it is used: the next {@link #startTone(int)} re-arms the output.
   */
  public native void release();

  native void nativeInit(int streamType, int volume);
}
