// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.res;

import picodroid.graphics.Theme;
import picodroid.util.DisplayMetrics;

/**
 * The app's compiled {@code res/} tree, addressed by the ids of its generated {@code R} class.
 * Mirrors {@code android.content.res.Resources}; obtain it from {@link
 * picodroid.content.Context#getResources()}.
 *
 * <p>Gradle compiles {@code res/values/*.xml}, {@code res/layout/*.xml} and {@code
 * res/drawable/*.png} into a binary table inside the app's PAPK, and generates {@code R.java} next
 * to the app's own sources. Lookups read that table in place; no XML is parsed on the device.
 *
 * <p>One density ({@code dp}, {@code sp} and {@code px} are all one pixel) and one locale, so
 * {@code values-night/} or {@code drawable-hdpi/} are a build error. What may vary is the window
 * and the input: {@code values-} and {@code layout-} directories qualified with {@code sw<N>dp},
 * {@code w<N>dp}, {@code h<N>dp}, {@code land} / {@code port} and {@code notouch} / {@code finger},
 * in that order, override the base values on a board that matches — chosen once when the app
 * starts, by the same {@link #getConfiguration() configuration} this class reports.
 */
public final class Resources {
  private static Resources sInstance;

  private DisplayMetrics mMetrics;

  private Resources() {}

  /**
   * The running app's resources. Not an Android API — apps call {@code Context.getResources()};
   * this is how {@code Context} reaches the singleton from another package.
   */
  public static Resources getInstance() {
    if (sInstance == null) {
      sInstance = new Resources();
    }
    return sInstance;
  }

  /**
   * Mirrors Android: the screen and input this app runs with — its window's size in dp, whether the
   * board has a touch panel and whether it has navigation keys. A fresh snapshot each call; nothing
   * in it changes while the app runs.
   */
  public Configuration getConfiguration() {
    return Configuration.current();
  }

  /**
   * Mirrors Android: the string for {@code R.string.*}.
   *
   * @throws NotFoundException if {@code id} is not a string resource of this app
   */
  public native String getString(int id);

  /**
   * Mirrors Android: the string {@code id} used as a format, filled with {@code formatArgs} as
   * {@code String.format} does.
   */
  public String getString(int id, Object... formatArgs) {
    return String.format(getString(id), formatArgs);
  }

  /** Mirrors Android: same as {@link #getString}; styled text does not exist here. */
  public CharSequence getText(int id) {
    return getString(id);
  }

  /**
   * Mirrors Android: the ARGB colour for {@code R.color.*}.
   *
   * @throws NotFoundException if {@code id} is not a colour resource of this app
   */
  public native int getColor(int id);

  /**
   * Mirrors Android: the dimension for {@code R.dimen.*}, in pixels.
   *
   * @throws NotFoundException if {@code id} is not a dimension resource of this app
   */
  public native float getDimension(int id);

  /** Mirrors Android: {@link #getDimension} rounded, and at least one pixel when non-zero. */
  public int getDimensionPixelSize(int id) {
    float f = getDimension(id);
    int px = Math.round(f);
    if (px != 0) {
      return px;
    }
    if (f == 0) {
      return 0;
    }
    return f > 0 ? 1 : -1;
  }

  /** Mirrors Android: {@link #getDimension} truncated to whole pixels. */
  public int getDimensionPixelOffset(int id) {
    return (int) getDimension(id);
  }

  /**
   * Mirrors Android: the integer for {@code R.integer.*}.
   *
   * @throws NotFoundException if {@code id} is not an integer resource of this app
   */
  public native int getInteger(int id);

  /**
   * Mirrors Android: the boolean for {@code R.bool.*}.
   *
   * @throws NotFoundException if {@code id} is not a boolean resource of this app
   */
  public native boolean getBoolean(int id);

  /**
   * Makes the style {@code R.style.*} the theme: the colours it names become the defaults of the
   * framework's own widgets ({@link Theme}). What {@code Context.setTheme} runs.
   *
   * <p>Only those colours live on the device. The rest of a theme is spent at build time: {@code
   * ?attr/…} in a layout is the value from the app's {@code AppTheme} style, and a view's {@code
   * style="@style/…"} is expanded into its attributes.
   *
   * @throws NotFoundException if {@code styleId} is not a style resource of this app
   */
  public void applyTheme(int styleId) {
    int words = nativeStyleWord(styleId, -1);
    for (int i = 0; i < words; i += 2) {
      int color = nativeStyleWord(styleId, i + 1);
      switch (nativeStyleWord(styleId, i)) {
        case 1: // THEME_COLOR_PRIMARY
          Theme.colorPrimary = color;
          break;
        case 2: // THEME_COLOR_ON_PRIMARY
          Theme.colorOnPrimary = color;
          break;
        case 3: // THEME_COLOR_BACKGROUND
          Theme.colorBackground = color;
          break;
        case 4: // THEME_COLOR_SURFACE
          Theme.colorSurface = color;
          break;
        case 5: // THEME_TEXT_COLOR_PRIMARY
          Theme.colorText = color;
          break;
        case 6: // THEME_TEXT_COLOR_SECONDARY
          Theme.colorTextSecondary = color;
          break;
        case 7: // THEME_COLOR_OUTLINE
          Theme.colorOutline = color;
          break;
        default:
          // A theme attribute from a newer compiler: nothing here reads it.
          break;
      }
    }
  }

  /** Word {@code index} of a style's stream; its length for {@code index} -1. */
  private static native int nativeStyleWord(int style, int index);

  /**
   * Mirrors Android: the display's size and density. One density, so {@link DisplayMetrics#density}
   * is 1 and a {@code dp} is a pixel; see {@link DisplayMetrics}.
   */
  public DisplayMetrics getDisplayMetrics() {
    if (mMetrics == null) {
      mMetrics = new DisplayMetrics();
      mMetrics.setToDefaults();
    }
    return mMetrics;
  }

  /** Mirrors Android: thrown when a requested resource id does not exist. */
  public static class NotFoundException extends RuntimeException {
    public NotFoundException() {}

    public NotFoundException(String name) {
      super(name);
    }
  }
}
