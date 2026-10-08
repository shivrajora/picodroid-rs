// SPDX-License-Identifier: GPL-3.0-only
package picodroid.provider;

import picodroid.content.ContentResolver;

/**
 * Mirrors the one table of {@code android.provider.Settings} this framework keeps: {@link System}
 * with {@link System#SCREEN_OFF_TIMEOUT}, stored on the volume at {@code /system/display} and read
 * by the display's idle timer. Any other name reads as its default and refuses a write.
 */
public final class Settings {
  private Settings() {}

  /** Mirrors {@code android.provider.Settings.System}. */
  public static final class System {
    /** Milliseconds of no input before the display dozes; {@code 0} is never. */
    public static final String SCREEN_OFF_TIMEOUT = "screen_off_timeout";

    private System() {}

    /** Mirrors Android: the setting's value, or {@code def} when it is not one kept here. */
    public static int getInt(ContentResolver cr, String name, int def) {
      return nativeGetInt(name, def);
    }

    /** Mirrors Android: store the setting; {@code false} for a name not kept here. */
    public static boolean putInt(ContentResolver cr, String name, int value) {
      return nativePutInt(name, value);
    }

    private static native int nativeGetInt(String name, int def);

    private static native boolean nativePutInt(String name, int value);
  }
}
