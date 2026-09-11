// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

/**
 * One alarm: a local wall-clock time, the weekdays it repeats on, and whether it is armed.
 *
 * <p>A mutable struct rather than an immutable value with setters — the edit screen rewrites fields
 * in place and {@link AlarmStore} holds a fixed array of these for the life of the process, so
 * every alarm edit on this board costs no allocation at all.
 */
public final class Alarm {
  /** {@link #daysMask} for an alarm that fires once and then disarms itself. */
  public static final int ONCE = 0;

  /** Bit {@code n} of {@link #daysMask} is the day {@link Clock#dayOfWeek} returns as {@code n}. */
  public static final int SUNDAY = 1;

  public static final int MONDAY = 1 << 1;
  public static final int TUESDAY = 1 << 2;
  public static final int WEDNESDAY = 1 << 3;
  public static final int THURSDAY = 1 << 4;
  public static final int FRIDAY = 1 << 5;
  public static final int SATURDAY = 1 << 6;
  public static final int EVERY_DAY = 0x7F;

  /** Stable index into {@link AlarmStore}, and the extra an Intent carries to the edit screen. */
  public final int id;

  public int hour;
  public int minute;
  public boolean enabled;
  public int daysMask;
  public String label;

  Alarm(int id) {
    this.id = id;
    hour = 7;
    minute = 0;
    enabled = false;
    daysMask = ONCE;
    label = "";
  }

  /** Whether this alarm repeats rather than firing once. */
  public boolean repeats() {
    return daysMask != ONCE;
  }

  /** "07:00". */
  public String time() {
    return Clock.hm(hour, minute);
  }

  /**
   * The repeat as a row reads it: "Once", "Every day", "Mon Tue Wed Thu Fri", and so on. Days come
   * out Monday-first, which is how the edit screen orders its toggles.
   */
  public String repeatText() {
    if (daysMask == ONCE) {
      return "Once";
    }
    if ((daysMask & EVERY_DAY) == EVERY_DAY) {
      return "Every day";
    }
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 7; i++) {
      int day = (i + 1) % 7; // Mon(1) .. Sat(6), Sun(0)
      if ((daysMask & (1 << day)) != 0) {
        if (sb.length() > 0) {
          sb.append(' ');
        }
        sb.append(NAMES[day]);
      }
    }
    return sb.toString();
  }

  private static final String[] NAMES = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
}
