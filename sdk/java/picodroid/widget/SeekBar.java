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

  /**
   * Mirrors {@code android.widget.SeekBar.OnSeekBarChangeListener}. Picodroid currently fires only
   * {@link #onProgressChanged}; {@link #onStartTrackingTouch} and {@link #onStopTrackingTouch}
   * default to no-op and are reserved for future LVGL press/release wiring.
   */
  public interface OnSeekBarChangeListener {
    void onProgressChanged(SeekBar seekBar, int progress, boolean fromUser);

    default void onStartTrackingTouch(SeekBar seekBar) {}

    default void onStopTrackingTouch(SeekBar seekBar) {}
  }
}
