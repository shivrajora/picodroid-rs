// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

public class Runtime {
  public static native long gcTimeNanos();

  public static native int gcCount();

  public static native int gcFreed();

  public static native void resetGcStats();

  /** Approximate bytes currently live across the object / array / string heaps. */
  public static native long usedMemory();

  /** Maximum {@link #usedMemory()} observed since the last {@link #resetPeakMemory()}. */
  public static native long peakMemory();

  /** Snap the peak counter to the current {@link #usedMemory()} value. */
  public static native void resetPeakMemory();
}
