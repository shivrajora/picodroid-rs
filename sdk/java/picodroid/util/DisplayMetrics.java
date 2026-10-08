// SPDX-License-Identifier: GPL-3.0-only
package picodroid.util;

import picodroid.graphics.Display;

/**
 * Mirrors {@code android.util.DisplayMetrics}: the display's size, density and pixel pitch, as
 * {@link picodroid.content.res.Resources#getDisplayMetrics()} hands it out. There is one density:
 * {@link #density} and {@link #scaledDensity} are 1 and {@link #densityDpi} is {@link
 * #DENSITY_DEFAULT}, so a {@code dp} or an {@code sp} is one logical pixel, the rule the layout
 * compiler applies to {@code res/} too. {@link #xdpi} and {@link #ydpi} are the panel's true pitch
 * (the board file's {@code [display] dpi}), so a point, inch or millimetre through {@link
 * TypedValue#applyDimension} is a ruler measurement, and {@code 7 mm} is a fingertip on every
 * board.
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
    Display.getInstance().getMetrics(this);
  }
}
