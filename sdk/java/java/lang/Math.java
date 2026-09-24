// SPDX-License-Identifier: GPL-3.0-only
package java.lang;

public final class Math {
  public static final double PI = 3.141592653589793;
  public static final double E = 2.718281828459045;

  public static native int abs(int a);

  public static native long abs(long a);

  public static native float abs(float a);

  public static native double abs(double a);

  public static native int min(int a, int b);

  public static native long min(long a, long b);

  public static native float min(float a, float b);

  public static native double min(double a, double b);

  public static native int max(int a, int b);

  public static native long max(long a, long b);

  public static native float max(float a, float b);

  public static native double max(double a, double b);

  public static native double sqrt(double a);

  public static native double pow(double a, double b);

  public static native double floor(double a);

  public static native double ceil(double a);

  public static native int round(float a);

  public static native long round(double a);

  public static native double sin(double a);

  public static native double cos(double a);

  public static native double tan(double a);

  public static native double atan2(double y, double x);

  public static native double toRadians(double deg);

  public static native double toDegrees(double rad);

  public static native double log(double a);

  public static native double log10(double a);

  public static native double exp(double a);

  // The bodied helpers below are plain Java: the interpreter runs them, no native arm needed.

  /** The largest {@code int} not greater than {@code x / y}; rounds toward negative infinity. */
  public static int floorDiv(int x, int y) {
    int r = x / y;
    if ((x ^ y) < 0 && (r * y != x)) {
      r--;
    }
    return r;
  }

  public static long floorDiv(long x, long y) {
    long r = x / y;
    if ((x ^ y) < 0 && (r * y != x)) {
      r--;
    }
    return r;
  }

  /** The remainder of {@link #floorDiv}: the sign of the result is the sign of {@code y}. */
  public static int floorMod(int x, int y) {
    return x - floorDiv(x, y) * y;
  }

  public static long floorMod(long x, long y) {
    return x - floorDiv(x, y) * y;
  }

  public static int addExact(int x, int y) {
    int r = x + y;
    if (((x ^ r) & (y ^ r)) < 0) {
      throw new ArithmeticException("integer overflow");
    }
    return r;
  }

  public static long addExact(long x, long y) {
    long r = x + y;
    if (((x ^ r) & (y ^ r)) < 0) {
      throw new ArithmeticException("long overflow");
    }
    return r;
  }

  public static int subtractExact(int x, int y) {
    int r = x - y;
    if (((x ^ y) & (x ^ r)) < 0) {
      throw new ArithmeticException("integer overflow");
    }
    return r;
  }

  public static long subtractExact(long x, long y) {
    long r = x - y;
    if (((x ^ y) & (x ^ r)) < 0) {
      throw new ArithmeticException("long overflow");
    }
    return r;
  }

  public static int multiplyExact(int x, int y) {
    long r = (long) x * (long) y;
    if ((int) r != r) {
      throw new ArithmeticException("integer overflow");
    }
    return (int) r;
  }

  public static long multiplyExact(long x, long y) {
    long r = x * y;
    long ax = Math.abs(x);
    long ay = Math.abs(y);
    if (((ax | ay) >>> 31 != 0)) {
      // Some bits greater than 2^31 that might cause overflow: check the division.
      if (((y != 0) && (r / y != x)) || (x == Long.MIN_VALUE && y == -1)) {
        throw new ArithmeticException("long overflow");
      }
    }
    return r;
  }

  public static int toIntExact(long value) {
    if ((int) value != value) {
      throw new ArithmeticException("integer overflow");
    }
    return (int) value;
  }
}
