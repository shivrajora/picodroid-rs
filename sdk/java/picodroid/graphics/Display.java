// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics;

import picodroid.util.DisplayMetrics;
import picodroid.view.View;

/**
 * Mirrors the size side of {@code android.view.Display}: the panel's logical size and, through
 * {@link #getMetrics}, its density and pixel pitch. One density — a {@code dp} is one logical pixel
 * on every board — so {@link DisplayMetrics#density} is 1 and {@link DisplayMetrics#densityDpi} is
 * 160; {@link DisplayMetrics#xdpi} and {@code ydpi} are the panel's real pitch from the board file,
 * which is what sizes a touch target or a millimetre.
 */
public class Display {
  private int width;
  private int height;
  // The panel's physical pixels per inch (`[display] dpi` in board.toml), set natively.
  private int dpi;

  private Display(int width, int height) {
    this.width = width;
    this.height = height;
  }

  public static native Display getInstance();

  public native void setContentView(View root);

  public native void update();

  public int getWidth() {
    return width;
  }

  public int getHeight() {
    return height;
  }

  /** Mirrors Android: fills {@code outMetrics} with this display's size, density and pitch. */
  public void getMetrics(DisplayMetrics outMetrics) {
    outMetrics.widthPixels = width;
    outMetrics.heightPixels = height;
    outMetrics.density = 1f;
    outMetrics.densityDpi = DisplayMetrics.DENSITY_DEFAULT;
    outMetrics.scaledDensity = 1f;
    outMetrics.xdpi = dpi;
    outMetrics.ydpi = dpi;
  }
}
