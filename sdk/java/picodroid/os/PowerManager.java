// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * Mirrors the display side of {@code android.os.PowerManager}, from {@link
 * picodroid.content.Context#getSystemService} with {@link picodroid.content.Context#POWER_SERVICE}:
 * whether the panel is on. The panel dozes after the screen timeout ({@code
 * Settings.System.SCREEN_OFF_TIMEOUT}; the board's default is 60 s) unless a view holds it with
 * {@link picodroid.view.View#setKeepScreenOn}, and wakes on a key, a touch, a {@code
 * KEYCODE_WAKEUP} or an Activity that set {@link picodroid.app.Activity#setTurnScreenOn}. The app
 * keeps running while the panel is dark. No wake locks: a dark panel does not stop the CPU here.
 */
public class PowerManager {
  private static final PowerManager INSTANCE = new PowerManager();

  private PowerManager() {}

  public static PowerManager getInstance() {
    return INSTANCE;
  }

  /** Mirrors Android: whether the display is on (not dozing). */
  public native boolean isInteractive();
}
