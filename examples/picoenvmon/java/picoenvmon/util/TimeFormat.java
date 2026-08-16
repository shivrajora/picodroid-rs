// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.util;

/**
 * Epoch-milliseconds → display strings, pure integer math (there is no java.util.Date/Calendar in
 * the SDK). Times are UTC plus a single build-time offset — no timezone database exists on this
 * platform; adjust {@link #UTC_OFFSET_MINUTES} for local display if desired.
 */
public final class TimeFormat {
  /** Displayed-time offset from UTC, in minutes. 0 = UTC (documented on the Network screen). */
  public static final int UTC_OFFSET_MINUTES = 0;

  private TimeFormat() {}

  /** "HH:MM:SS". */
  public static String hms(long epochMs) {
    long daySec = localDaySeconds(epochMs);
    return String.format(
        "%02d:%02d:%02d", (int) (daySec / 3600), (int) ((daySec % 3600) / 60), (int) (daySec % 60));
  }

  /** "HH:MM" — for tight History rows. */
  public static String hm(long epochMs) {
    long daySec = localDaySeconds(epochMs);
    return String.format("%02d:%02d", (int) (daySec / 3600), (int) ((daySec % 3600) / 60));
  }

  /** "YYYY-MM-DD HH:MM:SS". */
  public static String dateTime(long epochMs) {
    long adjusted = epochMs + UTC_OFFSET_MINUTES * 60_000L;
    long days = floorDiv(adjusted, 86_400_000L);
    int[] ymd = civilFromDays(days);
    return String.format("%04d-%02d-%02d ", ymd[0], ymd[1], ymd[2]) + hms(epochMs);
  }

  private static long localDaySeconds(long epochMs) {
    long adjusted = epochMs + UTC_OFFSET_MINUTES * 60_000L;
    long sec = floorDiv(adjusted, 1000L);
    long daySec = sec % 86_400L;
    if (daySec < 0) {
      daySec += 86_400L;
    }
    return daySec;
  }

  /**
   * Days-since-epoch → {year, month, day}. Howard Hinnant's civil_from_days algorithm — integer
   * only, exact over the int range.
   */
  private static int[] civilFromDays(long z) {
    z += 719_468L;
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

  private static long floorDiv(long a, long b) {
    long q = a / b;
    if ((a % b != 0) && ((a < 0) != (b < 0))) {
      q--;
    }
    return q;
  }
}
