// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

public class SystemClock {
  public static native void sleep(int ms);

  public static native long elapsedRealtimeNanos();

  /**
   * Milliseconds since boot, the coarse form of {@link #elapsedRealtimeNanos}. Unlike {@code
   * System.currentTimeMillis()} it never jumps: {@link #setCurrentTimeMillis} moves the wall clock
   * and leaves this alone, which is what makes it the right base for a delay.
   */
  public static long elapsedRealtime() {
    return elapsedRealtimeNanos() / 1000000L;
  }

  /**
   * Anchors the wall clock: after this call {@code System.currentTimeMillis()} returns real epoch
   * time (before any call it counts from boot). Typically fed from an SNTP sync. Always returns
   * {@code true} — Android's permission-denied case does not apply here.
   */
  /**
   * Anchor the wall clock: {@code System.currentTimeMillis()} reads {@code millis} now. The
   * platform's time service does this from the network once the link is up (and every few hours
   * after), so an app only calls it to set the clock by hand on a board with no network. Android
   * requires {@code SET_TIME}; permissions are not enforced here, and the call returns true.
   */
  public static native boolean setCurrentTimeMillis(long millis);
}
