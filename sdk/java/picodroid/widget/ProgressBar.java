// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class ProgressBar extends View {
  /** Determinate progress bar — call {@link #setProgress(int)} to update value 0..100. */
  public ProgressBar() {
    super(nativeCreate());
  }

  /**
   * Indeterminate progress indicator — a rotating arc that ignores {@link #setProgress(int)}.
   * Backed by LVGL's {@code lv_spinner}; the underlying widget type is fixed at construction so the
   * {@code nativeHandle} stays stable for the lifetime of the View.
   */
  public static ProgressBar indeterminate() {
    return new ProgressBar(nativeCreateIndeterminate());
  }

  private ProgressBar(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  private static native int nativeCreateIndeterminate();

  /** No-op when this ProgressBar is indeterminate. */
  public native void setProgress(int value);
}
