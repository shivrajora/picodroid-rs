// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.graphics.Theme;
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
   *
   * <p>The moving arc defaults to {@link Theme#colorPrimary} (rebrand by assigning {@code Theme}
   * fields before any UI is built). Override per-instance via {@link #setTint(int)} — mirrors
   * Android's {@code setIndeterminateTintList}.
   */
  public static ProgressBar indeterminate() {
    return new ProgressBar(nativeCreateIndeterminate(Theme.colorPrimary));
  }

  private ProgressBar(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  private static native int nativeCreateIndeterminate(int argb);

  /** No-op when this ProgressBar is indeterminate. */
  public native void setProgress(int value);

  /**
   * Tint the moving arc of an indeterminate ProgressBar. Silently no-ops on a determinate bar
   * (matches Android's {@code setIndeterminateTintList} which only affects the indeterminate
   * drawable). The track ring keeps its theme-derived gray.
   */
  public native void setTint(int argbColor);
}
