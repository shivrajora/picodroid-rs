// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.res;

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
 * <p>There are no resource configurations: one display, one density ({@code dp}, {@code sp} and
 * {@code px} are all one pixel) and one locale, so qualified directories such as {@code
 * values-night/} are a build error.
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
   * Mirrors Android: the string for {@code R.string.*}.
   *
   * @throws NotFoundException if {@code id} is not a string resource of this app
   */
  public native String getString(int id);

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
