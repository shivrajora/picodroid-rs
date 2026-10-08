// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.res;

import picodroid.content.pm.PackageManager;
import picodroid.graphics.Display;
import picodroid.util.DisplayMetrics;
import picodroid.view.KeyCharacterMap;
import picodroid.view.KeyEvent;

/**
 * Mirrors {@code android.content.res.Configuration}: the screen and the input the app is running
 * with, from {@link Resources#getConfiguration()}. The screen values are the app's window — the
 * design size an app declares with {@code <supports-screens>}, else the panel — in {@code dp},
 * which is a pixel here. The input values are the board's: {@link #touchscreen} is {@link
 * #TOUCHSCREEN_FINGER} where the board has a touch panel, {@link #navigation} is {@link
 * #NAVIGATION_DPAD} where it has the four navigation keys (then {@link
 * picodroid.view.View#isInTouchMode()} is false), and {@link #keyboard} is always {@link
 * #KEYBOARD_NOKEYS}: text is typed on the on-screen keyboard. There is one density ({@link
 * DisplayMetrics#DENSITY_DEFAULT}), one locale and no night mode, so those fields are absent.
 * Nothing changes while an app runs — no rotation, no {@code onConfigurationChanged} — so the
 * object is a snapshot to read, not to watch.
 *
 * <p>The design is <a
 * href="https://github.com/shivrajora/picodroid-rs/blob/main/docs/designs/app-portability-2026-10.md">app
 * portability</a>.
 */
public final class Configuration {
  /** Mirrors Android: {@link #orientation} not known. */
  public static final int ORIENTATION_UNDEFINED = 0;

  /** Mirrors Android: the window is at least as tall as it is wide. */
  public static final int ORIENTATION_PORTRAIT = 1;

  /** Mirrors Android: the window is wider than it is tall. */
  public static final int ORIENTATION_LANDSCAPE = 2;

  /** Mirrors Android: {@link #touchscreen} not known. */
  public static final int TOUCHSCREEN_UNDEFINED = 0;

  /** Mirrors Android: no touch panel; the keys drive the focus. */
  public static final int TOUCHSCREEN_NOTOUCH = 1;

  /** Mirrors Android: a finger-driven touch panel. */
  public static final int TOUCHSCREEN_FINGER = 3;

  /** Mirrors Android: {@link #navigation} not known. */
  public static final int NAVIGATION_UNDEFINED = 0;

  /** Mirrors Android: no navigation keys; a finger reaches everything. */
  public static final int NAVIGATION_NONAV = 1;

  /** Mirrors Android: UP / DOWN / SELECT keys move and activate the focus. */
  public static final int NAVIGATION_DPAD = 2;

  /** Mirrors Android: {@link #keyboard} not known. */
  public static final int KEYBOARD_UNDEFINED = 0;

  /** Mirrors Android: no text keys; the on-screen keyboard types. */
  public static final int KEYBOARD_NOKEYS = 1;

  /** Mirrors Android: {@link #densityDpi} not known. */
  public static final int DENSITY_DPI_UNDEFINED = 0;

  /** Mirrors Android: {@link #screenWidthDp} not known. */
  public static final int SCREEN_WIDTH_DP_UNDEFINED = 0;

  /** Mirrors Android: {@link #screenHeightDp} not known. */
  public static final int SCREEN_HEIGHT_DP_UNDEFINED = 0;

  /** Mirrors Android: {@link #smallestScreenWidthDp} not known. */
  public static final int SMALLEST_SCREEN_WIDTH_DP_UNDEFINED = 0;

  /** Mirrors Android: the window's width in dp (a pixel here). */
  public int screenWidthDp;

  /** Mirrors Android: the window's height in dp. */
  public int screenHeightDp;

  /** Mirrors Android: the shorter of the window's two sides, in dp. */
  public int smallestScreenWidthDp;

  /** Mirrors Android: the density bucket, always {@link DisplayMetrics#DENSITY_DEFAULT} here. */
  public int densityDpi;

  /** Mirrors Android: one of the {@code ORIENTATION_*} values. */
  public int orientation;

  /** Mirrors Android: one of the {@code TOUCHSCREEN_*} values. */
  public int touchscreen;

  /** Mirrors Android: one of the {@code NAVIGATION_*} values. */
  public int navigation;

  /** Mirrors Android: one of the {@code KEYBOARD_*} values. */
  public int keyboard;

  /** Mirrors Android: every field undefined; {@link Resources#getConfiguration()} fills one in. */
  public Configuration() {
    setToDefaults();
  }

  /** Mirrors Android: a copy of {@code o}. */
  public Configuration(Configuration o) {
    setTo(o);
  }

  /** Mirrors Android: copies every field of {@code o} into this configuration. */
  public void setTo(Configuration o) {
    screenWidthDp = o.screenWidthDp;
    screenHeightDp = o.screenHeightDp;
    smallestScreenWidthDp = o.smallestScreenWidthDp;
    densityDpi = o.densityDpi;
    orientation = o.orientation;
    touchscreen = o.touchscreen;
    navigation = o.navigation;
    keyboard = o.keyboard;
  }

  /** Mirrors Android: resets every field to its {@code *_UNDEFINED} value. */
  public void setToDefaults() {
    screenWidthDp = SCREEN_WIDTH_DP_UNDEFINED;
    screenHeightDp = SCREEN_HEIGHT_DP_UNDEFINED;
    smallestScreenWidthDp = SMALLEST_SCREEN_WIDTH_DP_UNDEFINED;
    densityDpi = DENSITY_DPI_UNDEFINED;
    orientation = ORIENTATION_UNDEFINED;
    touchscreen = TOUCHSCREEN_UNDEFINED;
    navigation = NAVIGATION_UNDEFINED;
    keyboard = KEYBOARD_UNDEFINED;
  }

  /**
   * Mirrors Android: whether the window is at least {@code w} by {@code h} dp — the test a layout
   * makes before choosing a two-column arrangement.
   */
  public boolean isLayoutSizeAtLeast(int w, int h) {
    return screenWidthDp >= w && screenHeightDp >= h;
  }

  /**
   * The configuration this app runs with. Not an Android API: {@link Resources#getConfiguration()}
   * is the way to it.
   */
  static Configuration current() {
    Configuration c = new Configuration();
    Display d = Display.getInstance();
    int w = d.getWidth();
    int h = d.getHeight();
    c.screenWidthDp = w;
    c.screenHeightDp = h;
    c.smallestScreenWidthDp = Math.min(w, h);
    c.densityDpi = DisplayMetrics.DENSITY_DEFAULT;
    c.orientation = w > h ? ORIENTATION_LANDSCAPE : ORIENTATION_PORTRAIT;
    c.touchscreen =
        PackageManager.getInstance().hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN)
            ? TOUCHSCREEN_FINGER
            : TOUCHSCREEN_NOTOUCH;
    // The four navigation keys come together; SELECT stands for them. The same board fact
    // View.isInTouchMode() reads, from the static side.
    c.navigation =
        KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_DPAD_CENTER)
            ? NAVIGATION_DPAD
            : NAVIGATION_NONAV;
    c.keyboard = KEYBOARD_NOKEYS;
    return c;
  }

  /**
   * Mirrors Android's shape: {@code {sw240dp w320dp h240dp 160dpi land finger dpad nokeys}}, with
   * {@code ?} for an undefined field.
   */
  @Override
  public String toString() {
    StringBuilder sb = new StringBuilder("{");
    sb.append("sw").append(smallestScreenWidthDp).append("dp");
    sb.append(" w").append(screenWidthDp).append("dp");
    sb.append(" h").append(screenHeightDp).append("dp");
    sb.append(' ').append(densityDpi).append("dpi");
    sb.append(' ')
        .append(
            orientation == ORIENTATION_LANDSCAPE
                ? "land"
                : orientation == ORIENTATION_PORTRAIT ? "port" : "?orien");
    sb.append(' ')
        .append(
            touchscreen == TOUCHSCREEN_FINGER
                ? "finger"
                : touchscreen == TOUCHSCREEN_NOTOUCH ? "notouch" : "?touch");
    sb.append(' ')
        .append(
            navigation == NAVIGATION_DPAD
                ? "dpad"
                : navigation == NAVIGATION_NONAV ? "nonav" : "?nav");
    sb.append(' ').append(keyboard == KEYBOARD_NOKEYS ? "nokeys" : "?keyb");
    return sb.append('}').toString();
  }
}
