// SPDX-License-Identifier: GPL-3.0-only
package kotlin.internal;

/**
 * Last element of a progression; kotlinc calls it inline for {@code for (i in a downTo b step s)}.
 */
public final class ProgressionUtilKt {
  private ProgressionUtilKt() {}

  private static int mod(int a, int b) {
    int m = a % b;
    return m >= 0 ? m : m + b;
  }

  private static int differenceModulo(int a, int b, int c) {
    return mod(mod(a, c) - mod(b, c), c);
  }

  public static int getProgressionLastElement(int start, int end, int step) {
    if (step > 0) {
      return start >= end ? end : end - differenceModulo(end, start, step);
    }
    if (step < 0) {
      return start <= end ? end : end + differenceModulo(start, end, -step);
    }
    throw new IllegalArgumentException("Step is zero.");
  }
}
