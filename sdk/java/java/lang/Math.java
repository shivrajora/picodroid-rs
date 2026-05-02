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
}
