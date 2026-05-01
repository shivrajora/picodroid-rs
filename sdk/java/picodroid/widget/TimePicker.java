package picodroid.widget;

import picodroid.view.View;

/**
 * Inline 24-hour time picker. Backed by two side-by-side LVGL {@code lv_roller}s — hour (0..23) and
 * minute (0..59), both wrapping infinitely. Either roller settling on a new value fires {@link
 * #setOnTimeChangedListener}.
 */
public class TimePicker extends View {
  private Runnable timeChangedListener;

  public TimePicker() {
    super(nativeCreate());
  }

  /** Set the displayed hour (0..23) and minute (0..59). */
  public native void setTime(int hour, int minute);

  public native int getHour();

  public native int getMinute();

  public void setOnTimeChangedListener(Runnable listener) {
    this.timeChangedListener = listener;
    nativeRegisterTimeChangedListener();
  }

  void fireTimeChanged() {
    if (timeChangedListener != null) {
      timeChangedListener.run();
    }
  }

  private static native int nativeCreate();

  private native void nativeRegisterTimeChangedListener();
}
