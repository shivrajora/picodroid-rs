// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics.drawable;

import picodroid.graphics.Color;
import picodroid.view.View;

/**
 * Configurable shape drawable: solid fill (or two-color linear gradient), optional corner radius,
 * optional stroke. All properties are independent — a 1px-stroke pill is just {@code
 * setColor(...).setCornerRadius(20).setStroke(1, Color.WHITE)}.
 *
 * <p>Mirrors the most-used subset of Android's {@code GradientDrawable}. Multi-stop gradients,
 * angle-arbitrary linear gradients, and radial gradients are deferred — LVGL's {@link
 * picodroid.graphics.drawable.GradientDrawable.Orientation#TOP_BOTTOM} and {@link
 * picodroid.graphics.drawable.GradientDrawable.Orientation#LEFT_RIGHT} cover the typical UI cases.
 */
public class GradientDrawable extends Drawable {
  /** Two-color gradient direction. Mirrors LV_GRAD_DIR_VER / LV_GRAD_DIR_HOR on the Rust side. */
  public static final class Orientation {
    public static final int TOP_BOTTOM = 1;
    public static final int LEFT_RIGHT = 2;

    private Orientation() {}
  }

  private int fillColor = Color.argb(255, 255, 255, 255);
  private int radius = 0;
  private int strokeWidth = 0;
  private int strokeColor = Color.argb(255, 0, 0, 0);
  private boolean hasGradient = false;
  private int gradientStart = 0;
  private int gradientEnd = 0;
  private int gradientOrientation = Orientation.TOP_BOTTOM;

  /** Solid fill color in {@code 0xAARRGGBB}. Replaces any previously set gradient. */
  public GradientDrawable setColor(int color) {
    this.fillColor = color;
    this.hasGradient = false;
    return this;
  }

  /** Corner radius in pixels. Pass a value at least half the smaller dimension to render a pill. */
  public GradientDrawable setCornerRadius(int radius) {
    this.radius = radius;
    return this;
  }

  /** Border outline. Pass {@code width = 0} to remove. */
  public GradientDrawable setStroke(int width, int color) {
    this.strokeWidth = width;
    this.strokeColor = color;
    return this;
  }

  /**
   * Two-color linear gradient. The {@code orientation} must be {@link Orientation#TOP_BOTTOM} or
   * {@link Orientation#LEFT_RIGHT}. Replaces any previously set solid color.
   */
  public GradientDrawable setGradient(int startColor, int endColor, int orientation) {
    this.hasGradient = true;
    this.gradientStart = startColor;
    this.gradientEnd = endColor;
    this.gradientOrientation = orientation;
    return this;
  }

  @Override
  public void applyTo(View v) {
    // The native side reads `v.nativeHandle` via the View field-slot
    // table — keeps GradientDrawable out of the picodroid.view package
    // (where `nativeHandle` is package-private).
    nativeApply(
        v,
        fillColor,
        radius,
        strokeWidth,
        strokeColor,
        hasGradient ? 1 : 0,
        gradientStart,
        gradientEnd,
        gradientOrientation);
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
