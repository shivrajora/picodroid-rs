// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

public class SeekBar extends View {
  private OnSeekBarChangeListener onSeekBarChangeListener;

  public SeekBar(Context ctx) {
    super(ctx, nativeCreate());
    setFocusable(true);
  }

  private static native int nativeCreate();

  /**
   * Sets the range's upper bound; a progress above it is pulled down to it, and the listener hears
   * that as a change with {@code fromUser == false}, as on Android.
   */
  public void setMax(int max) {
    int before = getProgress();
    nativeSetMax(max);
    if (getProgress() != before) {
      notifyProgressChanged(false);
    }
  }

  /**
   * Sets the progress (clamped to the range). A change reaches the listener with {@code fromUser ==
   * false}, so the "keep a label in sync" idiom holds for programmatic changes too; setting the
   * progress the bar already has reports nothing.
   */
  public void setProgress(int progress) {
    int before = getProgress();
    nativeSetProgress(progress);
    if (getProgress() != before) {
      notifyProgressChanged(false);
    }
  }

  private native void nativeSetMax(int max);

  private native void nativeSetProgress(int progress);

  public native int getProgress();

  /**
   * Synthetically advance the slider and fire a progress-change event for headless testing.
   * Registered listener runs on the next main-loop dispatch tick.
   */
  public native void performProgressChange();

  /**
   * Synthetically fire a press/release pair for headless testing — drives
   * onStartTrackingTouch/onStopTrackingTouch through the real LVGL event path.
   */
  public native void performTrackingTouch();

  public void setOnSeekBarChangeListener(OnSeekBarChangeListener listener) {
    this.onSeekBarChangeListener = listener;
    nativeRegisterChangeListener();
  }

  private native void nativeRegisterChangeListener();

  /** The framework's drag or key change: {@code fromUser == true}. */
  void fireProgressChanged() {
    notifyProgressChanged(true);
  }

  private void notifyProgressChanged(boolean fromUser) {
    if (onSeekBarChangeListener != null) {
      onSeekBarChangeListener.onProgressChanged(this, getProgress(), fromUser);
    }
  }

  /** Fans the LVGL press/release edge out to the tracking callbacks. */
  void fireTrackingTouch(boolean start) {
    if (onSeekBarChangeListener != null) {
      if (start) {
        onSeekBarChangeListener.onStartTrackingTouch(this);
      } else {
        onSeekBarChangeListener.onStopTrackingTouch(this);
      }
    }
  }

  /**
   * Mirrors {@code android.widget.SeekBar.OnSeekBarChangeListener}: {@link #onProgressChanged}
   * fires on value changes, {@link #onStartTrackingTouch}/{@link #onStopTrackingTouch} on the LVGL
   * press/release edges. The tracking methods keep default no-op bodies so single-method lambdas
   * remain valid.
   */
  public interface OnSeekBarChangeListener {
    void onProgressChanged(SeekBar seekBar, int progress, boolean fromUser);

    default void onStartTrackingTouch(SeekBar seekBar) {}

    default void onStopTrackingTouch(SeekBar seekBar) {}
  }
}
