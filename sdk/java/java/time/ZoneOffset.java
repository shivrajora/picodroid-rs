// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.zone.ZoneRules;

/** A fixed offset from UTC, {@code -18:00} to {@code +18:00}, as in the JDK. */
public final class ZoneOffset extends ZoneId implements Comparable<ZoneOffset> {
  private static final int MAX_SECONDS = 18 * 3600;

  public static final ZoneOffset UTC = new ZoneOffset(0);
  public static final ZoneOffset MIN = new ZoneOffset(-MAX_SECONDS);
  public static final ZoneOffset MAX = new ZoneOffset(MAX_SECONDS);

  private final int totalSeconds;
  private final String id;

  private ZoneOffset(int totalSeconds) {
    this.totalSeconds = totalSeconds;
    this.id = buildId(totalSeconds);
  }

  /**
   * Parses {@code Z}, {@code +h}, {@code +hh}, {@code +hh:mm}, {@code +hhmm}, {@code +hh:mm:ss} or
   * {@code +hhmmss}.
   */
  public static ZoneOffset of(String offsetId) {
    if (offsetId == null) {
      throw new NullPointerException("offsetId");
    }
    if (offsetId.equals("Z")) {
      return UTC;
    }
    int len = offsetId.length();
    if (len < 2) {
      throw new DateTimeException("Invalid ID for ZoneOffset, invalid format: " + offsetId);
    }
    char first = offsetId.charAt(0);
    if (first != '+' && first != '-') {
      throw new DateTimeException("Invalid ID for ZoneOffset, plus/minus not found: " + offsetId);
    }
    int hours;
    int minutes = 0;
    int seconds = 0;
    switch (len) {
      case 2:
        hours = digit(offsetId, 1);
        break;
      case 3:
        hours = twoDigits(offsetId, 1);
        break;
      case 5:
        hours = twoDigits(offsetId, 1);
        minutes = twoDigits(offsetId, 3);
        break;
      case 6:
        hours = twoDigits(offsetId, 1);
        minutes = twoDigits(offsetId, 4);
        colon(offsetId, 3);
        break;
      case 7:
        hours = twoDigits(offsetId, 1);
        minutes = twoDigits(offsetId, 3);
        seconds = twoDigits(offsetId, 5);
        break;
      case 9:
        hours = twoDigits(offsetId, 1);
        minutes = twoDigits(offsetId, 4);
        seconds = twoDigits(offsetId, 7);
        colon(offsetId, 3);
        colon(offsetId, 6);
        break;
      default:
        throw new DateTimeException("Invalid ID for ZoneOffset, invalid format: " + offsetId);
    }
    if (first == '-') {
      return ofHoursMinutesSeconds(-hours, -minutes, -seconds);
    }
    return ofHoursMinutesSeconds(hours, minutes, seconds);
  }

  private static void colon(String id, int pos) {
    if (id.charAt(pos) != ':') {
      throw new DateTimeException(
          "Invalid ID for ZoneOffset, colon not found when expected: " + id);
    }
  }

  private static int digit(String id, int pos) {
    char c = id.charAt(pos);
    if (c < '0' || c > '9') {
      throw new DateTimeException("Invalid ID for ZoneOffset, non numeric characters found: " + id);
    }
    return c - '0';
  }

  private static int twoDigits(String id, int pos) {
    return digit(id, pos) * 10 + digit(id, pos + 1);
  }

  public static ZoneOffset ofHours(int hours) {
    return ofHoursMinutesSeconds(hours, 0, 0);
  }

  public static ZoneOffset ofHoursMinutes(int hours, int minutes) {
    return ofHoursMinutesSeconds(hours, minutes, 0);
  }

  public static ZoneOffset ofHoursMinutesSeconds(int hours, int minutes, int seconds) {
    if (hours < -18 || hours > 18) {
      throw new DateTimeException("Zone offset hours not in valid range: " + hours);
    }
    if (hours > 0 && (minutes < 0 || seconds < 0)) {
      throw new DateTimeException("Zone offset minutes and seconds must be positive");
    }
    if (hours < 0 && (minutes > 0 || seconds > 0)) {
      throw new DateTimeException("Zone offset minutes and seconds must be negative");
    }
    if (minutes < -59 || minutes > 59 || seconds < -59 || seconds > 59) {
      throw new DateTimeException("Zone offset minutes or seconds not in valid range");
    }
    return ofTotalSeconds(hours * 3600 + minutes * 60 + seconds);
  }

  public static ZoneOffset ofTotalSeconds(int totalSeconds) {
    if (totalSeconds < -MAX_SECONDS || totalSeconds > MAX_SECONDS) {
      throw new DateTimeException("Zone offset not in valid range: -18:00 to +18:00");
    }
    if (totalSeconds == 0) {
      return UTC;
    }
    return new ZoneOffset(totalSeconds);
  }

  private static String buildId(int totalSecs) {
    if (totalSecs == 0) {
      return "Z";
    }
    int absTotal = Math.abs(totalSecs);
    int absHours = absTotal / 3600;
    int absMinutes = (absTotal / 60) % 60;
    int absSeconds = absTotal % 60;
    StringBuilder buf = new StringBuilder();
    buf.append(totalSecs < 0 ? '-' : '+');
    twoDigits(buf, absHours);
    buf.append(':');
    twoDigits(buf, absMinutes);
    if (absSeconds != 0) {
      buf.append(':');
      twoDigits(buf, absSeconds);
    }
    return buf.toString();
  }

  private static void twoDigits(StringBuilder buf, int v) {
    if (v < 10) {
      buf.append('0');
    }
    buf.append(v);
  }

  public int getTotalSeconds() {
    return totalSeconds;
  }

  @Override
  public String getId() {
    return id;
  }

  @Override
  public ZoneRules getRules() {
    return ZoneRules.of(this);
  }

  @Override
  public int compareTo(ZoneOffset other) {
    return other.totalSeconds - totalSeconds;
  }

  @Override
  public boolean equals(Object obj) {
    return obj instanceof ZoneOffset && ((ZoneOffset) obj).totalSeconds == totalSeconds;
  }

  @Override
  public int hashCode() {
    return totalSeconds;
  }

  @Override
  public String toString() {
    return id;
  }
}
