// SPDX-License-Identifier: GPL-3.0-only
package picodroid.provider;

import picodroid.content.ContentResolver;

/**
 * The system settings, the shape of {@code android.provider.Settings}: {@link System} and {@link
 * Global} tables read and written by name through a {@link ContentResolver}, which the framework
 * does not consult (there is one settings store). Two settings exist: the screen timeout and
 * automatic time. Any other name reads as its default and refuses writes.
 */
public final class Settings {
  private Settings() {}

  /** Per-device UI settings. */
  public static final class System {
    /**
     * The screen timeout in milliseconds, {@code 0} for never (Settings → Display; the display's
     * idle timer reads it at once).
     */
    public static final String SCREEN_OFF_TIMEOUT = "screen_off_timeout";

    private System() {}

    public static int getInt(ContentResolver cr, String name, int def) {
      return nativeGetInt(name, def);
    }

    public static boolean putInt(ContentResolver cr, String name, int value) {
      return nativePutInt(name, value);
    }

    private static native int nativeGetInt(String name, int def);

    private static native boolean nativePutInt(String name, int value);
  }

  /** Device-wide settings. */
  public static final class Global {
    /**
     * Whether the platform sets the wall clock from the network: {@code 1} (the default) and the
     * time service anchors it from {@code pool.ntp.org} once the link is up and every few hours
     * after; {@code 0} and the clock is only what Settings → Date &amp; time or an app sets. The
     * zone is not a setting here: {@code TimeZone.getDefault()} reads it, {@code
     * AlarmManager.setTimeZone} writes it.
     */
    public static final String AUTO_TIME = "auto_time";

    private Global() {}

    public static int getInt(ContentResolver cr, String name, int def) {
      return nativeGetInt(name, def);
    }

    public static boolean putInt(ContentResolver cr, String name, int value) {
      return nativePutInt(name, value);
    }

    private static native int nativeGetInt(String name, int def);

    private static native boolean nativePutInt(String name, int value);
  }
}
