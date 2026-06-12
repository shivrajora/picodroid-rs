// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.graphics.Theme;
import picodroid.view.View;

public class ProgressBar extends View {
  /**
   * Current progress, cached Java-side so {@link #getProgress()} returns the set value immediately
   * (the LVGL bar animates toward it over a few frames) — same Android-semantics caching as the
   * View state getters. Stays 0 while indeterminate.
   */
  private int progress;

  /** Fixed at construction: LVGL can't morph an lv_bar into an lv_spinner after creation. */
  private boolean indeterminate;

  /** Determinate progress bar — call {@link #setProgress(int)} to update value 0..100. */
  public ProgressBar() {
    super(nativeCreate());
  }

  public ProgressBar(Context ctx) {
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
    ProgressBar bar = new ProgressBar(nativeCreateIndeterminate(Theme.colorPrimary));
    bar.indeterminate = true;
    return bar;
  }

  private ProgressBar(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  private static native int nativeCreateIndeterminate(int argb);

  /** No-op when this ProgressBar is indeterminate, matching Android. */
  public void setProgress(int value) {
    if (indeterminate) {
      return;
    }
    progress = value;
    nativeSetProgress(value);
  }

  private native void nativeSetProgress(int value);

  /**
   * Mirrors {@code android.widget.ProgressBar#getProgress()}: returns the most recent {@link
   * #setProgress(int)} value, or 0 while indeterminate (indeterminate bars ignore setProgress).
   */
  public int getProgress() {
    return progress;
  }

  /**
   * Mirrors {@code android.widget.ProgressBar#isIndeterminate()}. Picodroid divergence: the mode is
   * fixed at construction ({@link #ProgressBar()} vs {@link #indeterminate()}) — there is no {@code
   * setIndeterminate(boolean)}, because the backing LVGL widget type (bar vs spinner) cannot change
   * after creation.
   */
  public boolean isIndeterminate() {
    return indeterminate;
  }

  /**
   * Tint the moving arc of an indeterminate ProgressBar. Silently no-ops on a determinate bar
   * (matches Android's {@code setIndeterminateTintList} which only affects the indeterminate
   * drawable). The track ring keeps its theme-derived gray.
   */
  public native void setTint(int argbColor);
}
