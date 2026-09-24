// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.util;

import java.time.Duration;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.TimeZone;

/** The screens' short forms of a clock time, a duration, a token count and a price. */
public final class TimeFormat {
  private static final DateTimeFormatter HM = DateTimeFormatter.ofPattern("HH:mm");

  private TimeFormat() {}

  /**
   * Installs the bridge's UTC offset as the zone every local time is shown in: the PC knows its
   * timezone, the device does not, and {@code java.time} here has no tz database — one fixed offset
   * is the whole of a zone.
   */
  public static void setUtcOffsetMinutes(int minutes) {
    TimeZone.setDefault(TimeZone.getTimeZone(ZoneOffset.ofTotalSeconds(minutes * 60)));
  }

  /** Local "12:03". */
  public static String hm(long epochMs) {
    return LocalDateTime.ofInstant(Instant.ofEpochMilli(epochMs), ZoneId.systemDefault())
        .format(HM);
  }

  /** "3d 4h", "2h 14m", "14m", "<1m": two units at most, so it stays short at any scale. */
  public static String duration(long ms) {
    Duration d = Duration.ofMillis(ms);
    long minutes = d.toMinutes();
    if (minutes < 1) {
      return "<1m";
    }
    long hours = d.toHours();
    long days = d.toDays();
    if (days > 0) {
      return String.format("%dd %dh", days, hours % 24);
    }
    if (hours > 0) {
      return String.format("%dh %dm", hours, minutes % 60);
    }
    return minutes + "m";
  }

  /** Thousands of tokens as "840K" or "1.84M". */
  public static String tokens(int thousands) {
    if (thousands < 1000) {
      return thousands + "K";
    }
    int hundredths = thousands / 10; // of a million
    return String.format("%d.%02dM", hundredths / 100, hundredths % 100);
  }

  /** Cents as "$12.30". */
  public static String dollars(int cents) {
    return String.format("$%d.%02d", cents / 100, cents % 100);
  }
}
