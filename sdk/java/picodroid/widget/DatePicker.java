// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * Inline calendar-style date picker. Backed by LVGL's {@code lv_calendar}.
 *
 * <p>Tap a day cell to fire {@link #setOnDateChangedListener}; the selected year/month/day are read
 * back via {@link #getYear()}, {@link #getMonth()}, {@link #getDay()}.
 */
public class DatePicker extends View {
  private OnDateChangedListener dateChangedListener;

  public DatePicker() {
    super(nativeCreate());
  }

  public DatePicker(Context ctx) {
    super(nativeCreate());
  }

  /** Sets today's date and the month being shown. {@code month} is 1..12, {@code day} is 1..31. */
  public native void setDate(int year, int month, int day);

  /** Returns the most-recently-selected year, or 0 if no day has been tapped yet. */
  public native int getYear();

  /** Returns the selected month (1..12), or 0 if no day has been tapped yet. */
  public native int getMonth();

  /** Returns the selected day (1..31), or 0 if no day has been tapped yet. */
  public native int getDay();

  public void setOnDateChangedListener(OnDateChangedListener listener) {
    this.dateChangedListener = listener;
    nativeRegisterDateChangedListener();
  }

  void fireDateChanged() {
    if (dateChangedListener != null) {
      dateChangedListener.onDateChanged(this, getYear(), getMonth(), getDay());
    }
  }

  private static native int nativeCreate();

  private native void nativeRegisterDateChangedListener();

  /** Mirrors {@code android.widget.DatePicker.OnDateChangedListener}. */
  public interface OnDateChangedListener {
    void onDateChanged(DatePicker view, int year, int monthOfYear, int dayOfMonth);
  }
}
