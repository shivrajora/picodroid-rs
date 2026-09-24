// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.format.DateTimeFormatter;
import java.time.temporal.ChronoUnit;
import java.time.temporal.Temporal;
import java.time.temporal.TemporalAmount;
import java.time.temporal.TemporalUnit;

/** A time of day without a date or zone, to nanosecond precision, as in the JDK. */
public final class LocalTime implements Temporal, Comparable<LocalTime> {
  public static final LocalTime MIN = new LocalTime(0, 0, 0, 0);
  public static final LocalTime MAX = new LocalTime(23, 59, 59, 999_999_999);
  public static final LocalTime MIDNIGHT = MIN;
  public static final LocalTime NOON = new LocalTime(12, 0, 0, 0);

  static final int HOURS_PER_DAY = 24;
  static final int MINUTES_PER_HOUR = 60;
  static final int MINUTES_PER_DAY = 1440;
  static final int SECONDS_PER_MINUTE = 60;
  static final int SECONDS_PER_HOUR = 3600;
  static final int SECONDS_PER_DAY = 86400;
  static final long MILLIS_PER_DAY = 86_400_000L;
  static final long NANOS_PER_SECOND = 1_000_000_000L;
  static final long NANOS_PER_MINUTE = 60_000_000_000L;
  static final long NANOS_PER_HOUR = 3_600_000_000_000L;
  static final long NANOS_PER_DAY = 86_400_000_000_000L;

  private final int hour;
  private final int minute;
  private final int second;
  private final int nano;

  private LocalTime(int hour, int minute, int second, int nanoOfSecond) {
    this.hour = hour;
    this.minute = minute;
    this.second = second;
    this.nano = nanoOfSecond;
  }

  private static LocalTime create(int hour, int minute, int second, int nanoOfSecond) {
    if ((minute | second | nanoOfSecond) == 0) {
      if (hour == 0) {
        return MIDNIGHT;
      }
      if (hour == 12) {
        return NOON;
      }
    }
    return new LocalTime(hour, minute, second, nanoOfSecond);
  }

  /** The current time in the default zone ({@code ZoneId.systemDefault()}). */
  public static LocalTime now() {
    return now(ZoneId.systemDefault());
  }

  public static LocalTime now(ZoneId zone) {
    return LocalDateTime.now(zone).toLocalTime();
  }

  public static LocalTime of(int hour, int minute) {
    checkHour(hour);
    if (minute == 0) {
      return create(hour, 0, 0, 0);
    }
    checkMinute(minute);
    return new LocalTime(hour, minute, 0, 0);
  }

  public static LocalTime of(int hour, int minute, int second) {
    checkHour(hour);
    checkMinute(minute);
    checkSecond(second);
    return create(hour, minute, second, 0);
  }

  public static LocalTime of(int hour, int minute, int second, int nanoOfSecond) {
    checkHour(hour);
    checkMinute(minute);
    checkSecond(second);
    checkNano(nanoOfSecond);
    return create(hour, minute, second, nanoOfSecond);
  }

  public static LocalTime ofSecondOfDay(long secondOfDay) {
    if (secondOfDay < 0 || secondOfDay >= SECONDS_PER_DAY) {
      throw new DateTimeException("Invalid value for SecondOfDay: " + secondOfDay);
    }
    int hours = (int) (secondOfDay / SECONDS_PER_HOUR);
    secondOfDay -= (long) hours * SECONDS_PER_HOUR;
    int minutes = (int) (secondOfDay / SECONDS_PER_MINUTE);
    secondOfDay -= (long) minutes * SECONDS_PER_MINUTE;
    return create(hours, minutes, (int) secondOfDay, 0);
  }

  public static LocalTime ofNanoOfDay(long nanoOfDay) {
    if (nanoOfDay < 0 || nanoOfDay >= NANOS_PER_DAY) {
      throw new DateTimeException("Invalid value for NanoOfDay: " + nanoOfDay);
    }
    int hours = (int) (nanoOfDay / NANOS_PER_HOUR);
    nanoOfDay -= hours * NANOS_PER_HOUR;
    int minutes = (int) (nanoOfDay / NANOS_PER_MINUTE);
    nanoOfDay -= minutes * NANOS_PER_MINUTE;
    int seconds = (int) (nanoOfDay / NANOS_PER_SECOND);
    nanoOfDay -= seconds * NANOS_PER_SECOND;
    return create(hours, minutes, seconds, (int) nanoOfDay);
  }

  /** Parses {@code HH:mm}, {@code HH:mm:ss} or {@code HH:mm:ss.SSSSSSSSS} (up to nine digits). */
  public static LocalTime parse(CharSequence text) {
    String s = text.toString();
    try {
      int len = s.length();
      if (len < 5 || s.charAt(2) != ':') {
        throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalTime");
      }
      int hour = Integer.parseInt(s.substring(0, 2));
      int minute = Integer.parseInt(s.substring(3, 5));
      int second = 0;
      int nanos = 0;
      if (len > 5) {
        if (len < 8 || s.charAt(5) != ':') {
          throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalTime");
        }
        second = Integer.parseInt(s.substring(6, 8));
        if (len > 8) {
          if (s.charAt(8) != '.' || len > 18) {
            throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalTime");
          }
          int digits = len - 9;
          nanos = Integer.parseInt(s.substring(9));
          for (int i = digits; i < 9; i++) {
            nanos *= 10;
          }
        }
      }
      return of(hour, minute, second, nanos);
    } catch (NumberFormatException e) {
      throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalTime");
    }
  }

  private static void checkHour(int hour) {
    if (hour < 0 || hour > 23) {
      throw new DateTimeException("Invalid value for HourOfDay: " + hour);
    }
  }

  private static void checkMinute(int minute) {
    if (minute < 0 || minute > 59) {
      throw new DateTimeException("Invalid value for MinuteOfHour: " + minute);
    }
  }

  private static void checkSecond(int second) {
    if (second < 0 || second > 59) {
      throw new DateTimeException("Invalid value for SecondOfMinute: " + second);
    }
  }

  private static void checkNano(int nano) {
    if (nano < 0 || nano > 999_999_999) {
      throw new DateTimeException("Invalid value for NanoOfSecond: " + nano);
    }
  }

  static LocalTime from(Temporal temporal) {
    if (temporal instanceof LocalTime) {
      return (LocalTime) temporal;
    }
    if (temporal instanceof LocalDateTime) {
      return ((LocalDateTime) temporal).toLocalTime();
    }
    throw new DateTimeException("Unable to obtain LocalTime from " + temporal);
  }

  public int getHour() {
    return hour;
  }

  public int getMinute() {
    return minute;
  }

  public int getSecond() {
    return second;
  }

  public int getNano() {
    return nano;
  }

  @Override
  public boolean isSupported(TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      return unit.isTimeBased();
    }
    return unit != null && unit.isSupportedBy(this);
  }

  public LocalTime withHour(int hour) {
    if (this.hour == hour) {
      return this;
    }
    checkHour(hour);
    return create(hour, minute, second, nano);
  }

  public LocalTime withMinute(int minute) {
    if (this.minute == minute) {
      return this;
    }
    checkMinute(minute);
    return create(hour, minute, second, nano);
  }

  public LocalTime withSecond(int second) {
    if (this.second == second) {
      return this;
    }
    checkSecond(second);
    return create(hour, minute, second, nano);
  }

  public LocalTime withNano(int nanoOfSecond) {
    if (this.nano == nanoOfSecond) {
      return this;
    }
    checkNano(nanoOfSecond);
    return create(hour, minute, second, nanoOfSecond);
  }

  /** Truncates to {@code unit}: {@code truncatedTo(ChronoUnit.MINUTES)} zeroes the seconds. */
  public LocalTime truncatedTo(TemporalUnit unit) {
    if (unit == ChronoUnit.NANOS) {
      return this;
    }
    Duration unitDur = unit.getDuration();
    if (unitDur.getSeconds() > SECONDS_PER_DAY) {
      throw new DateTimeException("Unit is too large to be used for truncation");
    }
    long dur = unitDur.toNanos();
    if ((NANOS_PER_DAY % dur) != 0) {
      throw new DateTimeException("Unit must divide into a standard day without remainder");
    }
    long nod = toNanoOfDay();
    return ofNanoOfDay((nod / dur) * dur);
  }

  @Override
  public LocalTime plus(long amountToAdd, TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      switch ((ChronoUnit) unit) {
        case NANOS:
          return plusNanos(amountToAdd);
        case MICROS:
          return plusNanos((amountToAdd % 86_400_000_000L) * 1000);
        case MILLIS:
          return plusNanos((amountToAdd % MILLIS_PER_DAY) * 1_000_000);
        case SECONDS:
          return plusSeconds(amountToAdd);
        case MINUTES:
          return plusMinutes(amountToAdd);
        case HOURS:
          return plusHours(amountToAdd);
        case HALF_DAYS:
          return plusHours((amountToAdd % 2) * 12);
        default:
          throw new DateTimeException("Unsupported unit: " + unit);
      }
    }
    return unit.addTo(this, amountToAdd);
  }

  @Override
  public LocalTime minus(long amountToSubtract, TemporalUnit unit) {
    return amountToSubtract == Long.MIN_VALUE
        ? plus(Long.MAX_VALUE, unit).plus(1, unit)
        : plus(-amountToSubtract, unit);
  }

  /** As the JDK: {@code time.plus(Duration.ofMinutes(5))}, wrapping around midnight. */
  @Override
  public LocalTime plus(TemporalAmount amount) {
    return (LocalTime) amount.addTo(this);
  }

  @Override
  public LocalTime minus(TemporalAmount amount) {
    return (LocalTime) amount.subtractFrom(this);
  }

  /** Wraps around midnight, as in the JDK. */
  public LocalTime plusHours(long hoursToAdd) {
    if (hoursToAdd == 0) {
      return this;
    }
    int newHour = ((int) (hoursToAdd % HOURS_PER_DAY) + hour + HOURS_PER_DAY) % HOURS_PER_DAY;
    return create(newHour, minute, second, nano);
  }

  public LocalTime plusMinutes(long minutesToAdd) {
    if (minutesToAdd == 0) {
      return this;
    }
    int mofd = hour * MINUTES_PER_HOUR + minute;
    int newMofd =
        ((int) (minutesToAdd % MINUTES_PER_DAY) + mofd + MINUTES_PER_DAY) % MINUTES_PER_DAY;
    if (mofd == newMofd) {
      return this;
    }
    int newHour = newMofd / MINUTES_PER_HOUR;
    int newMinute = newMofd % MINUTES_PER_HOUR;
    return create(newHour, newMinute, second, nano);
  }

  public LocalTime plusSeconds(long secondsToAdd) {
    if (secondsToAdd == 0) {
      return this;
    }
    int sofd = hour * SECONDS_PER_HOUR + minute * SECONDS_PER_MINUTE + second;
    int newSofd =
        ((int) (secondsToAdd % SECONDS_PER_DAY) + sofd + SECONDS_PER_DAY) % SECONDS_PER_DAY;
    if (sofd == newSofd) {
      return this;
    }
    int newHour = newSofd / SECONDS_PER_HOUR;
    int newMinute = (newSofd / SECONDS_PER_MINUTE) % MINUTES_PER_HOUR;
    int newSecond = newSofd % SECONDS_PER_MINUTE;
    return create(newHour, newMinute, newSecond, nano);
  }

  public LocalTime plusNanos(long nanosToAdd) {
    if (nanosToAdd == 0) {
      return this;
    }
    long nofd = toNanoOfDay();
    long newNofd = ((nanosToAdd % NANOS_PER_DAY) + nofd + NANOS_PER_DAY) % NANOS_PER_DAY;
    if (nofd == newNofd) {
      return this;
    }
    return ofNanoOfDay(newNofd);
  }

  public LocalTime minusHours(long hoursToSubtract) {
    return plusHours(-(hoursToSubtract % HOURS_PER_DAY));
  }

  public LocalTime minusMinutes(long minutesToSubtract) {
    return plusMinutes(-(minutesToSubtract % MINUTES_PER_DAY));
  }

  public LocalTime minusSeconds(long secondsToSubtract) {
    return plusSeconds(-(secondsToSubtract % SECONDS_PER_DAY));
  }

  public LocalTime minusNanos(long nanosToSubtract) {
    return plusNanos(-(nanosToSubtract % NANOS_PER_DAY));
  }

  @Override
  public long until(Temporal endExclusive, TemporalUnit unit) {
    LocalTime end = from(endExclusive);
    if (unit instanceof ChronoUnit) {
      long nanosUntil = end.toNanoOfDay() - toNanoOfDay();
      switch ((ChronoUnit) unit) {
        case NANOS:
          return nanosUntil;
        case MICROS:
          return nanosUntil / 1000;
        case MILLIS:
          return nanosUntil / 1_000_000;
        case SECONDS:
          return nanosUntil / NANOS_PER_SECOND;
        case MINUTES:
          return nanosUntil / NANOS_PER_MINUTE;
        case HOURS:
          return nanosUntil / NANOS_PER_HOUR;
        case HALF_DAYS:
          return nanosUntil / (12 * NANOS_PER_HOUR);
        default:
          throw new DateTimeException("Unsupported unit: " + unit);
      }
    }
    return unit.between(this, end);
  }

  public LocalDateTime atDate(LocalDate date) {
    return LocalDateTime.of(date, this);
  }

  public int toSecondOfDay() {
    return hour * SECONDS_PER_HOUR + minute * SECONDS_PER_MINUTE + second;
  }

  public long toNanoOfDay() {
    return hour * NANOS_PER_HOUR + minute * NANOS_PER_MINUTE + second * NANOS_PER_SECOND + nano;
  }

  public String format(DateTimeFormatter formatter) {
    return formatter.format(this);
  }

  @Override
  public int compareTo(LocalTime other) {
    int cmp = hour - other.hour;
    if (cmp == 0) {
      cmp = minute - other.minute;
      if (cmp == 0) {
        cmp = second - other.second;
        if (cmp == 0) {
          cmp = nano - other.nano;
        }
      }
    }
    return cmp;
  }

  public boolean isAfter(LocalTime other) {
    return compareTo(other) > 0;
  }

  public boolean isBefore(LocalTime other) {
    return compareTo(other) < 0;
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (obj instanceof LocalTime) {
      LocalTime other = (LocalTime) obj;
      return hour == other.hour
          && minute == other.minute
          && second == other.second
          && nano == other.nano;
    }
    return false;
  }

  @Override
  public int hashCode() {
    long nod = toNanoOfDay();
    return (int) (nod ^ (nod >>> 32));
  }

  /** ISO-8601, as the JDK prints it: {@code 12:03}, {@code 12:03:09}, {@code 12:03:09.5}. */
  @Override
  public String toString() {
    StringBuilder buf = new StringBuilder(18);
    appendTo(buf);
    return buf.toString();
  }

  void appendTo(StringBuilder buf) {
    twoDigits(buf, hour);
    buf.append(':');
    twoDigits(buf, minute);
    if (second > 0 || nano > 0) {
      buf.append(':');
      twoDigits(buf, second);
      if (nano > 0) {
        buf.append('.');
        if (nano % 1_000_000 == 0) {
          buf.append(Integer.toString((nano / 1_000_000) + 1000).substring(1));
        } else if (nano % 1000 == 0) {
          buf.append(Integer.toString((nano / 1000) + 1_000_000).substring(1));
        } else {
          buf.append(Integer.toString(nano + 1_000_000_000).substring(1));
        }
      }
    }
  }

  static void twoDigits(StringBuilder buf, int v) {
    if (v < 10) {
      buf.append('0');
    }
    buf.append(v);
  }
}
