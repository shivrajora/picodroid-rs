// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

/**
 * Wall-clock arithmetic: epoch milliseconds to and from a local civil date, and the display strings
 * the screens show. Integer only — there is no {@code java.util.Date}, {@code Calendar} or {@code
 * TimeZone} on this platform, and no timezone database either, so "local" here means UTC shifted by
 * a single offset the user picks on the Set-time screen.
 *
 * <p>The board has no battery-backed RTC. {@code System.currentTimeMillis()} counts from whatever
 * {@link picodroid.os.SystemClock#setCurrentTimeMillis} last set, and from zero after a cold boot,
 * which is why {@link #isSet} exists: the clock screen says "clock not set" rather than showing
 * 1970 as if it meant something.
 */
public final class Clock {
  public static final long MS_PER_SECOND = 1000L;
  public static final long MS_PER_MINUTE = 60_000L;
  public static final long MS_PER_HOUR = 3_600_000L;
  public static final long MS_PER_DAY = 86_400_000L;

  /**
   * Epoch ms below which the clock is considered unset: 2001-01-01. Any real sync or hand-set puts
   * it far above this, and a cold boot leaves it far below.
   */
  private static final long SET_THRESHOLD_MS = 978_307_200_000L;

  private static final String[] WEEKDAY = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
  private static final String[] MONTH = {
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
  };

  private Clock() {}

  /** Whether the wall clock has been set since the last cold boot. */
  public static boolean isSet(long utcMs) {
    return utcMs >= SET_THRESHOLD_MS;
  }

  // ── UTC ↔ local ────────────────────────────────────────────────────────────

  /** UTC epoch ms to the local epoch ms that {@code offsetMinutes} east of UTC reads. */
  public static long toLocal(long utcMs, int offsetMinutes) {
    return utcMs + offsetMinutes * MS_PER_MINUTE;
  }

  /** The inverse of {@link #toLocal}. */
  public static long toUtc(long localMs, int offsetMinutes) {
    return localMs - offsetMinutes * MS_PER_MINUTE;
  }

  // ── Civil calendar ─────────────────────────────────────────────────────────

  /** Whole days since 1970-01-01 containing {@code localMs}, negative before the epoch. */
  public static long dayOf(long localMs) {
    return floorDiv(localMs, MS_PER_DAY);
  }

  /** Milliseconds since local midnight, always in [0, {@link #MS_PER_DAY}). */
  public static long msIntoDay(long localMs) {
    return localMs - dayOf(localMs) * MS_PER_DAY;
  }

  /** Local midnight of the day containing {@code localMs}, as local epoch ms. */
  public static long startOfDay(long localMs) {
    return dayOf(localMs) * MS_PER_DAY;
  }

  /** Hour of the day, 0-23. */
  public static int hourOf(long localMs) {
    return (int) (msIntoDay(localMs) / MS_PER_HOUR);
  }

  /** Minute of the hour, 0-59. */
  public static int minuteOf(long localMs) {
    return (int) (msIntoDay(localMs) % MS_PER_HOUR / MS_PER_MINUTE);
  }

  /** Second of the minute, 0-59. */
  public static int secondOf(long localMs) {
    return (int) (msIntoDay(localMs) % MS_PER_MINUTE / MS_PER_SECOND);
  }

  /**
   * Day of the week of an epoch day: 0 = Sunday through 6 = Saturday, matching the bit positions in
   * {@link Alarm#daysMask}. Day 0 (1970-01-01) was a Thursday, hence the +4.
   */
  public static int dayOfWeek(long epochDay) {
    return (int) floorMod(epochDay + 4, 7);
  }

  /**
   * Epoch day to {year, month, day}, month and day one-based. Howard Hinnant's civil_from_days:
   * integer only, exact, and proleptic Gregorian in both directions.
   */
  public static int[] civilFromDay(long epochDay) {
    long z = epochDay + 719_468L;
    long era = floorDiv(z, 146_097L);
    long doe = z - era * 146_097L; // [0, 146096]
    long yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    long y = yoe + era * 400;
    long doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    long mp = (5 * doy + 2) / 153; // [0, 11]
    long d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    long m = mp < 10 ? mp + 3 : mp - 9; // [1, 12]
    if (m <= 2) {
      y += 1;
    }
    return new int[] {(int) y, (int) m, (int) d};
  }

  /** {@link #civilFromDay} inverted: a civil date to its epoch day, month and day one-based. */
  public static long dayFromCivil(int year, int month, int day) {
    long y = year;
    if (month <= 2) {
      y -= 1;
    }
    long era = floorDiv(y, 400L);
    long yoe = y - era * 400; // [0, 399]
    long mp = month > 2 ? month - 3 : month + 9; // [0, 11]
    long doy = (153 * mp + 2) / 5 + day - 1; // [0, 365]
    long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    return era * 146_097L + doe - 719_468L;
  }

  /** Days in {@code month} of {@code year}, month one-based — what the date picker clamps to. */
  public static int daysInMonth(int year, int month) {
    return (int)
        (dayFromCivil(month == 12 ? year + 1 : year, month == 12 ? 1 : month + 1, 1)
            - dayFromCivil(year, month, 1));
  }

  // ── Display ────────────────────────────────────────────────────────────────

  /** "HH:MM" in 24-hour form. */
  public static String hm(int hour, int minute) {
    return two(hour) + ":" + two(minute);
  }

  /** "SS". */
  public static String ss(long localMs) {
    return two(secondOf(localMs));
  }

  /** "Thu 10 Sep 2026". */
  public static String date(long localMs) {
    int[] ymd = civilFromDay(dayOf(localMs));
    return WEEKDAY[dayOfWeek(dayOf(localMs))]
        + " "
        + ymd[2]
        + " "
        + MONTH[ymd[1] - 1]
        + " "
        + ymd[0];
  }

  /** A UTC offset as "UTC", "UTC+5:30" or "UTC-8". */
  public static String offset(int offsetMinutes) {
    if (offsetMinutes == 0) {
      return "UTC";
    }
    int magnitude = offsetMinutes < 0 ? -offsetMinutes : offsetMinutes;
    String s = "UTC" + (offsetMinutes < 0 ? "-" : "+") + magnitude / 60;
    return magnitude % 60 == 0 ? s : s + ":" + two(magnitude % 60);
  }

  /**
   * A duration as the countdown the alarm rows show: "in 7 min", "in 3 h 20 min", "in 2 days".
   * Rounds down, so "in 0 min" never appears — under a minute reads "in under a minute".
   */
  public static String until(long deltaMs) {
    if (deltaMs < MS_PER_MINUTE) {
      return "in under a minute";
    }
    long minutes = deltaMs / MS_PER_MINUTE;
    if (minutes < 60) {
      return "in " + minutes + " min";
    }
    long hours = minutes / 60;
    if (hours < 24) {
      long rest = minutes % 60;
      return rest == 0 ? "in " + hours + " h" : "in " + hours + " h " + rest + " min";
    }
    long days = hours / 24;
    return days == 1 ? "in 1 day" : "in " + days + " days";
  }

  /** Zero-padded to two digits; callers only ever pass 0-99. */
  public static String two(int v) {
    return v < 10 ? "0" + v : "" + v;
  }

  // ── Floor division ─────────────────────────────────────────────────────────
  // java.lang.Math has no floorDiv/floorMod here, and both have to round toward
  // negative infinity or every date before 1970 comes out a day off.

  public static long floorDiv(long a, long b) {
    long q = a / b;
    if (a % b != 0 && (a < 0) != (b < 0)) {
      q--;
    }
    return q;
  }

  public static long floorMod(long a, long b) {
    return a - floorDiv(a, b) * b;
  }
}
