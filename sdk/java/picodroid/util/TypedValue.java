// SPDX-License-Identifier: GPL-3.0-only
package picodroid.util;

/**
 * Mirrors the unit side of {@code android.util.TypedValue}: the {@code COMPLEX_UNIT_*} constants
 * that {@link picodroid.widget.TextView#setTextSize(int, float)} takes, and {@link
 * #applyDimension}, which turns a value in one of them into pixels. There is one density here
 * ({@code px}, {@code dp} and {@code sp} are the same pixel), so those three convert exactly;
 * points, inches and millimetres scale by {@link DisplayMetrics#xdpi}, which is the nominal 160,
 * not the panel's true pitch.
 */
public final class TypedValue {
  public static final int COMPLEX_UNIT_PX = 0;
  public static final int COMPLEX_UNIT_DIP = 1;
  public static final int COMPLEX_UNIT_SP = 2;
  public static final int COMPLEX_UNIT_PT = 3;
  public static final int COMPLEX_UNIT_IN = 4;
  public static final int COMPLEX_UNIT_MM = 5;

  private TypedValue() {}

  /** Mirrors Android: {@code value} in {@code unit} as pixels; 0 for a unit that is not one. */
  public static float applyDimension(int unit, float value, DisplayMetrics metrics) {
    switch (unit) {
      case COMPLEX_UNIT_PX:
        return value;
      case COMPLEX_UNIT_DIP:
        return value * metrics.density;
      case COMPLEX_UNIT_SP:
        return value * metrics.scaledDensity;
      case COMPLEX_UNIT_PT:
        return value * metrics.xdpi * (1.0f / 72);
      case COMPLEX_UNIT_IN:
        return value * metrics.xdpi;
      case COMPLEX_UNIT_MM:
        return value * metrics.xdpi * (1.0f / 25.4f);
      default:
        return 0;
    }
  }
}
