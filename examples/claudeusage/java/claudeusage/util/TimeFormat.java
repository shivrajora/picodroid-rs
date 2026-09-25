// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.util;

import java.time.Duration;
import java.time.LocalTime;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;

/**
 * The screens' short forms of a clock time, a duration, a token count and a price.
 *
 * <p>Deliberately touches four {@code java.time} classes and no more ({@code LocalTime}, {@code
 * ZoneOffset}, {@code Duration}, {@code DateTimeFormatter}): every class an app calls is parsed
 * into RAM on first use, about 5 KB each in the simulator's model (3 KB on the RP2350), and this
 * app runs within a few KB of the modelled heap (gaps roadmap G11). {@code
 * LocalDateTime.ofInstant(Instant.ofEpochMilli(ms), ZoneId.systemDefault())} would read better and
 * cost four more.
 */
public final class TimeFormat {
  private static final DateTimeFormatter HH_MM = DateTimeFormatter.ofPattern("HH:mm");

  /** The bridge's zone: the PC knows its timezone, the device does not. UTC until told. */
  private static ZoneOffset zone = ZoneOffset.UTC;

  private TimeFormat() {}

  /** Installs the bridge's UTC offset as the zone every local time is shown in. */
  public static void setUtcOffsetMinutes(int minutes) {
    zone = ZoneOffset.ofTotalSeconds(minutes * 60);
  }

  /** Local "12:03". */
  public static String hm(long epochMs) {
    long local = Math.floorDiv(epochMs, 1000L) + zone.getTotalSeconds();
    return LocalTime.ofSecondOfDay(Math.floorMod(local, 86_400L)).format(HH_MM);
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
