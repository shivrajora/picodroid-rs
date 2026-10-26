// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics.drawable;

import picodroid.annotation.Hide;
import picodroid.graphics.Color;
import picodroid.view.View;

/**
 * Mirrors {@code android.graphics.drawable.GradientDrawable}, the rectangle subset: a solid fill or
 * a two-colour linear gradient, a corner radius and a stroke, each independent. A 1 px pill:
 *
 * <pre>{@code
 * GradientDrawable pill = new GradientDrawable();
 * pill.setColor(Color.BLACK);
 * pill.setCornerRadius(20);
 * pill.setStroke(1, Color.WHITE);
 * view.setBackground(pill);
 * }</pre>
 *
 * <p>A gradient takes the first and last of {@link #setColors}'s colours along one of the four
 * axis-aligned {@link Orientation}s; no diagonal, radial or sweep gradients, no other shapes, no
 * per-corner radii.
 */
public class GradientDrawable extends Drawable {
  /**
   * Mirrors Android's {@code GradientDrawable.Orientation}, the four axis-aligned directions. A
   * final class of constants rather than an enum (no {@code values()} or {@code switch}): an enum's
   * tables would cost flash on every board.
   */
  public static final class Orientation {
    /** Top to bottom. */
    public static final Orientation TOP_BOTTOM = new Orientation(1, false);

    /** Bottom to top. */
    public static final Orientation BOTTOM_TOP = new Orientation(1, true);

    /** Left to right. */
    public static final Orientation LEFT_RIGHT = new Orientation(2, false);

    /** Right to left. */
    public static final Orientation RIGHT_LEFT = new Orientation(2, true);

    /** LV_GRAD_DIR_VER (1) / LV_GRAD_DIR_HOR (2) on the Rust side. */
    final int dir;

    /** The colours run end to start along {@link #dir}. */
    final boolean reversed;

    private Orientation(int dir, boolean reversed) {
      this.dir = dir;
      this.reversed = reversed;
    }
  }

  private int fillColor = Color.argb(255, 255, 255, 255);
  private int radius = 0;
  private int strokeWidth = 0;
  private int strokeColor = Color.argb(255, 0, 0, 0);
  private int[] colors;
  private Orientation orientation = Orientation.TOP_BOTTOM;

  /** A white rectangle; Android's starts transparent, so set a colour either way. */
  public GradientDrawable() {}

  /** Mirrors Android: a gradient through {@code colors} along {@code orientation}. */
  public GradientDrawable(Orientation orientation, int[] colors) {
    this.orientation = orientation == null ? Orientation.TOP_BOTTOM : orientation;
    this.colors = colors;
  }

  /** Solid fill in {@code 0xAARRGGBB}; replaces any gradient. */
  public void setColor(int argb) {
    this.fillColor = argb;
    this.colors = null;
  }

  /**
   * The gradient's colours, start to end; the first and last are drawn. One colour is a solid fill;
   * null drops the gradient for the {@link #setColor} fill.
   */
  public void setColors(int[] colors) {
    this.colors = colors;
  }

  /** The gradient's colours, or null for a solid fill. */
  public int[] getColors() {
    return colors;
  }

  /** The gradient's direction. */
  public void setOrientation(Orientation orientation) {
    if (orientation != null) {
      this.orientation = orientation;
    }
  }

  public Orientation getOrientation() {
    return orientation;
  }

  /**
   * Corner radius in pixels, rounded to the nearest; at least half the smaller side draws a pill.
   */
  public void setCornerRadius(float radius) {
    this.radius = Math.round(radius);
  }

  /** The outline; {@code width} 0 removes it. */
  public void setStroke(int width, int color) {
    this.strokeWidth = width;
    this.strokeColor = color;
  }

  @Override
  @Hide("view.setBackground(drawable)")
  public void applyTo(View v) {
    // The native side reads `v.nativeHandle` via the View field-slot table, which keeps
    // GradientDrawable out of the picodroid.view package (where `nativeHandle` is package-private).
    int[] c = colors;
    boolean gradient = c != null && c.length > 1;
    int fill = c != null && c.length == 1 ? c[0] : fillColor;
    int start = gradient ? c[orientation.reversed ? c.length - 1 : 0] : 0;
    int end = gradient ? c[orientation.reversed ? 0 : c.length - 1] : 0;
    nativeApply(
        v, fill, radius, strokeWidth, strokeColor, gradient ? 1 : 0, start, end, orientation.dir);
  }

  private static native void nativeApply(
      View target,
      int fillColor,
      int radius,
      int strokeWidth,
      int strokeColor,
      int hasGradient,
      int gradientStart,
      int gradientEnd,
      int gradientOrientation);
}
