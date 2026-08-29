// SPDX-License-Identifier: GPL-3.0-only
package kotlin.math;

/**
 * The {@code kotlin.math} functions that are not inline to {@code java.lang.Math} ({@code abs},
 * {@code sqrt}, {@code pow}, {@code floor}, {@code ceil}, {@code sin}, …, {@code ln}, {@code
 * log10}, {@code exp} are).
 */
public final class MathKt {
  private MathKt() {}

  private static final double LN2 = 0.6931471805599453;

  // Java's Math.round is floor(x + 0.5) (-2.5 -> -2); pico-jvm's builtin rounds half away from
  // zero (libm), so the shim spells the Java rule out to stay identical on both.
  // `value != value` is the NaN test; Float.isNaN(F) / Double.isNaN(D) are not pico-jvm builtins.
  @SuppressWarnings("IdentityBinaryExpression")
  public static int roundToInt(float value) {
    if (value != value) {
      throw new IllegalArgumentException("Cannot round NaN value.");
    }
    return (int) Math.floor(value + 0.5f);
  }

  @SuppressWarnings("IdentityBinaryExpression")
  public static int roundToInt(double value) {
    if (value != value) {
      throw new IllegalArgumentException("Cannot round NaN value.");
    }
    if (value > Integer.MAX_VALUE) {
      return Integer.MAX_VALUE;
    }
    if (value < Integer.MIN_VALUE) {
      return Integer.MIN_VALUE;
    }
    return (int) Math.floor(value + 0.5);
  }

  @SuppressWarnings("IdentityBinaryExpression")
  public static long roundToLong(double value) {
    if (value != value) {
      throw new IllegalArgumentException("Cannot round NaN value.");
    }
    return (long) Math.floor(value + 0.5);
  }

  public static double truncate(double value) {
    return value < 0 ? Math.ceil(value) : Math.floor(value);
  }

  public static double log2(double x) {
    return Math.log(x) / LN2;
  }

  public static double log(double x, double base) {
    if (base <= 0.0 || base == 1.0) {
      return Double.NaN;
    }
    return Math.log(x) / Math.log(base);
  }

  /** {@code Int.sign}. */
  public static int getSign(int value) {
    return value > 0 ? 1 : value < 0 ? -1 : 0;
  }
}
