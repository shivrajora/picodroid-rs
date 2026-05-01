package picodroid.widget;

import picodroid.view.View;

/**
 * Inline time picker. Backed by three side-by-side LVGL {@code lv_roller}s — hour, minute, and an
 * AM/PM column that's hidden in 24-hour mode (the default). Either roller settling on a new value
 * fires {@link #setOnTimeChangedListener}.
 *
 * <p>The internal {@link #getHour()} / {@link #setTime(int, int)} contract always uses 0..23 hour
 * values, regardless of display mode — matches Android's {@code TimePicker} API 23+.
 */
public class TimePicker extends View {
  private Runnable timeChangedListener;

  public TimePicker() {
    super(nativeCreate());
  }

  /** Set the underlying time. {@code hour} is always 0..23 regardless of display mode. */
  public native void setTime(int hour, int minute);

  /** Returns the underlying hour 0..23, regardless of display mode. */
  public native int getHour();

  public native int getMinute();

  /**
   * Toggle between 24-hour ({@code true}, the default) and 12-hour ({@code false}) display.
   * Preserves the underlying time across the switch — e.g. switching at 13:30 lands on 1:30 PM.
   */
  public native void setIs24HourView(boolean is24Hour);

  public native boolean is24HourView();

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
