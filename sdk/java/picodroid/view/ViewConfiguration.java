// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * The timings and thresholds the input pipeline uses, as {@code android.view.ViewConfiguration}
 * exposes them. The values are fixed per build: the key ones are mirrored by the native dispatcher
 * ({@code graphics/lvgl/key_repeat.rs}), which is where they take effect.
 */
public class ViewConfiguration {
  /** Android's {@code DEFAULT_LONG_PRESS_TIMEOUT}. */
  private static final int LONG_PRESS_TIMEOUT_MS = 400;

  /** Android's {@code KEY_REPEAT_DELAY}. */
  private static final int KEY_REPEAT_DELAY_MS = 50;

  private ViewConfiguration() {}

  /**
   * How long a key (or a touch) must be held to count as a long-press, in milliseconds. Also the
   * time to a held key's first auto-repeat, the one that carries {@link KeyEvent#FLAG_LONG_PRESS}.
   */
  public static int getLongPressTimeout() {
    return LONG_PRESS_TIMEOUT_MS;
  }

  /** The time from a key's press to its first auto-repeat, in milliseconds. */
  public static int getKeyRepeatTimeout() {
    return getLongPressTimeout();
  }

  /** The time between a held key's auto-repeats, in milliseconds. */
  public static int getKeyRepeatDelay() {
    return KEY_REPEAT_DELAY_MS;
  }
}
