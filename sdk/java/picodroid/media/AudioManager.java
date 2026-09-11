// SPDX-License-Identifier: GPL-3.0-only
package picodroid.media;

/**
 * Audio stream identifiers, mirroring the {@code STREAM_} constants on {@code
 * android.media.AudioManager}.
 *
 * <p>This class carries the constants and nothing else. picodroid has no mixer, no per-stream
 * volume and no audio policy: a board's audio output is a single buzzer, and {@link ToneGenerator}
 * drives it directly. The constants exist so that a {@link ToneGenerator} is constructed the way it
 * is on Android, spelling the stream rather than a bare integer, and so that the system-service
 * methods Android puts here have an obvious home if picodroid ever grows them.
 *
 * <p>Every stream behaves identically, which is to say the stream argument is ignored.
 */
public final class AudioManager {
  /** The voice call stream. */
  public static final int STREAM_VOICE_CALL = 0;

  /** The system sounds stream — the usual choice for {@link ToneGenerator}. */
  public static final int STREAM_SYSTEM = 1;

  /** The ringtone stream. */
  public static final int STREAM_RING = 2;

  /** The media playback stream. */
  public static final int STREAM_MUSIC = 3;

  /** The alarm stream. */
  public static final int STREAM_ALARM = 4;

  /** The notification stream. */
  public static final int STREAM_NOTIFICATION = 5;

  /** The DTMF stream. */
  public static final int STREAM_DTMF = 8;

  /** The accessibility stream. */
  public static final int STREAM_ACCESSIBILITY = 10;

  private AudioManager() {}
}
