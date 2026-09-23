// SPDX-License-Identifier: GPL-3.0-only
package picodroid.util;

import picodroid.graphics.Display;

/**
 * Mirrors {@code android.util.DisplayMetrics}: the display's size and density, as {@link
 * picodroid.content.res.Resources#getDisplayMetrics()} hands it out. There is one density: {@link
 * #density} and {@link #scaledDensity} are 1 and {@link #densityDpi} is {@link #DENSITY_DEFAULT},
 * so a {@code dp} or an {@code sp} is a pixel, the rule the layout compiler applies to {@code res/}
 * too. {@link #xdpi} and {@link #ydpi} are the same nominal 160 rather than the panel's true pitch,
 * so a point, inch or millimetre through {@link TypedValue#applyDimension} is a size in a 160 dpi
 * world, not a ruler measurement.
 */
public class DisplayMetrics {
  /** Mirrors Android: the reference density, one {@code dp} per pixel. */
  public static final int DENSITY_DEFAULT = 160;

  public int widthPixels;
  public int heightPixels;
  public float density;
  public int densityDpi;
  public float scaledDensity;
  public float xdpi;
  public float ydpi;

  /** Mirrors Android: fills in the one configuration this display has. */
  public void setToDefaults() {
    Display display = Display.getInstance();
    widthPixels = display.getWidth();
    heightPixels = display.getHeight();
    density = 1f;
    densityDpi = DENSITY_DEFAULT;
    scaledDensity = 1f;
    xdpi = DENSITY_DEFAULT;
    ydpi = DENSITY_DEFAULT;
  }
}
