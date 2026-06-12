// SPDX-License-Identifier: GPL-3.0-only
package picodroid.util;

/**
 * API for sending log output, mirroring {@code android.util.Log}: the {@link #v}/{@link #d}/{@link
 * #i}/{@link #w}/{@link #e} severity ladder plus {@link #wtf}, each with a {@code Throwable}
 * overload that appends the throwable's message.
 *
 * <p>On hardware the level maps to the matching defmt severity (RTT viewers can filter on it); the
 * simulator prints every level in the same {@code [Tag] message} line format.
 */
public class Log {
  public static native void v(String tag, String msg);

  public static native void d(String tag, String msg);

  public static native void i(String tag, String msg);

  public static native void w(String tag, String msg);

  public static native void e(String tag, String msg);

  /**
   * What a Terrible Failure: a condition that should never happen. Logs at error severity —
   * picodroid has no separate ASSERT channel.
   */
  public static void wtf(String tag, String msg) {
    e(tag, msg);
  }

  public static void v(String tag, String msg, Throwable tr) {
    v(tag, append(msg, tr));
  }

  public static void d(String tag, String msg, Throwable tr) {
    d(tag, append(msg, tr));
  }

  public static void i(String tag, String msg, Throwable tr) {
    i(tag, append(msg, tr));
  }

  public static void w(String tag, String msg, Throwable tr) {
    w(tag, append(msg, tr));
  }

  public static void e(String tag, String msg, Throwable tr) {
    e(tag, append(msg, tr));
  }

  public static void wtf(String tag, String msg, Throwable tr) {
    e(tag, append(msg, tr));
  }

  private static String append(String msg, Throwable tr) {
    if (tr == null) {
      return msg;
    }
    String m = tr.getMessage();
    return m == null ? msg : msg + ": " + m;
  }
}
