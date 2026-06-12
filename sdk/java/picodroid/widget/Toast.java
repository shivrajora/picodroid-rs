// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class Toast {
  public static final int LENGTH_SHORT = 0;
  public static final int LENGTH_LONG = 1;

  private final int nativeHandle;
  private int duration;

  private Toast(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  /**
   * Mirrors {@code Toast.makeText(Context, CharSequence, int)}. The {@code ctx} parameter matches
   * Android's signature; picodroid's single-Context model means it isn't consulted for placement
   * but the parameter is captured for future-proofing.
   */
  public static Toast makeText(Context ctx, String text, int duration) {
    Toast toast = new Toast(nativeCreate(text, duration));
    toast.duration = duration;
    return toast;
  }

  /** Mirrors {@code android.widget.Toast#getDuration()}. */
  public int getDuration() {
    return duration;
  }

  /**
   * Mirrors {@code android.widget.Toast#setDuration(int)} — {@link #LENGTH_SHORT} or {@link
   * #LENGTH_LONG}. Takes effect the next time {@link #show()} arms the auto-dismiss; a toast
   * already on screen keeps its original deadline.
   */
  public void setDuration(int duration) {
    this.duration = duration;
    nativeSetDuration(nativeHandle, duration);
  }

  public void show() {
    nativeShow(nativeHandle);
  }

  public void cancel() {
    nativeCancel(nativeHandle);
  }

  private static native int nativeCreate(String text, int duration);

  private static native void nativeShow(int nativeHandle);

  private static native void nativeCancel(int nativeHandle);

  private static native void nativeSetDuration(int nativeHandle, int duration);
}
