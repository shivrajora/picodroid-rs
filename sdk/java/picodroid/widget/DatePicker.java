// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * A calendar for picking a date, backed by {@code lv_calendar}. Mirrors {@code
 * android.widget.DatePicker}: {@link #updateDate} / {@link #init} set the date, {@link #getYear},
 * {@link #getMonth} and {@link #getDayOfMonth} read it, and the month is 0-based (January is 0)
 * everywhere, as on Android, so a ported {@code new GregorianCalendar(year, picker.getMonth(),
 * day)} lands on the month shown. The calendar shows 1 January 2026 until a date is set.
 */
public class DatePicker extends View {
  private OnDateChangedListener dateChangedListener;
  private int year = 2026;
  private int month; // 0-based, as Android's
  private int dayOfMonth = 1;

  public DatePicker() {
    this(null);
  }

  public DatePicker(Context ctx) {
    super(ctx, nativeCreate());
    nativeSetDate(year, month + 1, dayOfMonth);
  }

  /**
   * Shows {@code year}-{@code monthOfYear}-{@code dayOfMonth}, the month 0-based (January is 0).
   */
  public void updateDate(int year, int monthOfYear, int dayOfMonth) {
    if (monthOfYear < 0 || monthOfYear > 11) {
      throw new IllegalArgumentException("month out of range (0-11): " + monthOfYear);
    }
    this.year = year;
    this.month = monthOfYear;
    this.dayOfMonth = dayOfMonth;
    nativeSetDate(year, monthOfYear + 1, dayOfMonth);
  }

  /** {@link #updateDate} plus {@link #setOnDateChangedListener}, as Android's {@code init}. */
  public void init(int year, int monthOfYear, int dayOfMonth, OnDateChangedListener listener) {
    updateDate(year, monthOfYear, dayOfMonth);
    setOnDateChangedListener(listener);
  }

  /**
   * Picodroid's original setter, with a 1-based month (January is 1).
   *
   * @deprecated use {@link #updateDate}, whose month is 0-based like Android's.
   */
  @Deprecated
  public void setDate(int year, int month, int day) {
    updateDate(year, month - 1, day);
  }

  public int getYear() {
    return year;
  }

  /** The month shown, 0-based (January is 0), as on Android. */
  public int getMonth() {
    return month;
  }

  public int getDayOfMonth() {
    return dayOfMonth;
  }

  /**
   * @deprecated use {@link #getDayOfMonth}.
   */
  @Deprecated
  public int getDay() {
    return dayOfMonth;
  }

  public void setOnDateChangedListener(OnDateChangedListener listener) {
    this.dateChangedListener = listener;
    nativeRegisterDateChangedListener();
  }

  /** The framework's tap: read the date the calendar now shows, then tell the listener. */
  void fireDateChanged() {
    int tappedMonth = nativeGetMonth();
    if (tappedMonth >= 1) {
      year = nativeGetYear();
      month = tappedMonth - 1;
      dayOfMonth = nativeGetDay();
    }
    if (dateChangedListener != null) {
      dateChangedListener.onDateChanged(this, year, month, dayOfMonth);
    }
  }

  private static native int nativeCreate();

  /** The calendar's own convention: a 1-based month. */
  private native void nativeSetDate(int year, int month, int day);

  private native int nativeGetYear();

  private native int nativeGetMonth();

  private native int nativeGetDay();

  private native void nativeRegisterDateChangedListener();

  public interface OnDateChangedListener {
    /** {@code monthOfYear} is 0-based (January is 0), as on Android. */
    void onDateChanged(DatePicker view, int year, int monthOfYear, int dayOfMonth);
  }
}
