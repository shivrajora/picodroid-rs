// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

public class SeekBar extends View {
  private OnSeekBarChangeListener onSeekBarChangeListener;

  public SeekBar() {
    super(nativeCreate());
  }

  public SeekBar(int max) {
    super(nativeCreateWithMax(max));
  }

  public SeekBar(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  private static native int nativeCreateWithMax(int max);

  public native void setMax(int max);

  public native void setProgress(int progress);

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

  void fireProgressChanged() {
    if (onSeekBarChangeListener != null) {
      onSeekBarChangeListener.onProgressChanged(this, getProgress(), true);
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
