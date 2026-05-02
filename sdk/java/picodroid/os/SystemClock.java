// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

public class SystemClock {
  public static native void sleep(int ms);

  public static native long elapsedRealtimeNanos();
}
