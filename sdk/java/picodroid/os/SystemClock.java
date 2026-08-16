// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

public class SystemClock {
  public static native void sleep(int ms);

  public static native long elapsedRealtimeNanos();

  /**
   * Anchors the wall clock: after this call {@code System.currentTimeMillis()} returns real epoch
   * time (before any call it counts from boot). Typically fed from an SNTP sync. Always returns
   * {@code true} — Android's permission-denied case does not apply here.
   */
  public static native boolean setCurrentTimeMillis(long millis);
}
