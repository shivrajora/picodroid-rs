// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.util;

/** The SDK has no Date or Calendar; this is the integer arithmetic the screens need. */
public final class TimeFormat {
  /** Local offset from UTC, as reported by the bridge (the PC knows its timezone; we do not). */
  public static int utcOffsetMinutes;

  private TimeFormat() {}

  /** Local "12:03". */
  public static String hm(long epochMs) {
    long sec = epochMs / 1000L + utcOffsetMinutes * 60L;
    long daySec = sec % 86_400L;
    if (daySec < 0) {
      daySec += 86_400L;
    }
    return two((int) (daySec / 3600)) + ":" + two((int) ((daySec % 3600) / 60));
  }

  /** "3d 4h", "2h 14m", "14m", "<1m": two units at most, so it stays short at any scale. */
  public static String duration(long ms) {
    if (ms < 60_000L) {
      return "<1m";
    }
    long minutes = ms / 60_000L;
    long hours = minutes / 60;
    long days = hours / 24;
    if (days > 0) {
      return days + "d " + (hours % 24) + "h";
    }
    if (hours > 0) {
      return hours + "h " + (minutes % 60) + "m";
    }
    return minutes + "m";
  }

  /** Thousands of tokens as "840K" or "1.84M". */
  public static String tokens(int thousands) {
    if (thousands < 1000) {
      return thousands + "K";
    }
    int hundredths = thousands / 10; // of a million
    return (hundredths / 100) + "." + two(hundredths % 100) + "M";
  }

  /** Cents as "$12.30". */
  public static String dollars(int cents) {
    return "$" + (cents / 100) + "." + two(cents % 100);
  }

  private static String two(int v) {
    return v < 10 ? "0" + v : String.valueOf(v);
  }
}
