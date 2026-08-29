// SPDX-License-Identifier: GPL-3.0-only
package kotlin.ranges;

/** Progression factories and {@code coerce*}. {@code ..} itself is {@code new IntRange}. */
public final class RangesKt {
  private RangesKt() {}

  public static IntRange until(int from, int to) {
    if (to <= Integer.MIN_VALUE) {
      return new IntRange(1, 0);
    }
    return new IntRange(from, to - 1);
  }

  public static IntProgression downTo(int from, int to) {
    return new IntProgression(from, to, -1);
  }

  public static IntProgression step(IntProgression progression, int step) {
    if (step <= 0) {
      throw new IllegalArgumentException("Step must be positive, was: " + step + ".");
    }
    return new IntProgression(
        progression.getFirst(), progression.getLast(), progression.getStep() > 0 ? step : -step);
  }

  public static IntProgression reversed(IntProgression progression) {
    return new IntProgression(
        progression.getLast(), progression.getFirst(), -progression.getStep());
  }

  public static int coerceIn(int value, int min, int max) {
    if (min > max) {
      throw new IllegalArgumentException(
          "Cannot coerce value to an empty range: maximum "
              + max
              + " is less than minimum "
              + min
              + ".");
    }
    return value < min ? min : value > max ? max : value;
  }

  public static long coerceIn(long value, long min, long max) {
    if (min > max) {
      throw new IllegalArgumentException("Cannot coerce value to an empty range.");
    }
    return value < min ? min : value > max ? max : value;
  }

  public static float coerceIn(float value, float min, float max) {
    if (min > max) {
      throw new IllegalArgumentException("Cannot coerce value to an empty range.");
    }
    return value < min ? min : value > max ? max : value;
  }

  public static double coerceIn(double value, double min, double max) {
    if (min > max) {
      throw new IllegalArgumentException("Cannot coerce value to an empty range.");
    }
    return value < min ? min : value > max ? max : value;
  }

  public static int coerceIn(int value, ClosedRange<Integer> range) {
    if (range.isEmpty()) {
      throw new IllegalArgumentException("Cannot coerce value to an empty range: " + range + ".");
    }
    int lo = range.getStart().intValue();
    int hi = range.getEndInclusive().intValue();
    return value < lo ? lo : value > hi ? hi : value;
  }

  public static int coerceAtLeast(int value, int min) {
    return value < min ? min : value;
  }

  public static int coerceAtMost(int value, int max) {
    return value > max ? max : value;
  }
}
