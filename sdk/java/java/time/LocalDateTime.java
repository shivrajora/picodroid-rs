// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.chrono.ChronoLocalDateTime;
import java.time.format.DateTimeFormatter;
import java.time.temporal.ChronoUnit;
import java.time.temporal.Temporal;
import java.time.temporal.TemporalAmount;
import java.time.temporal.TemporalUnit;

/**
 * A date-time without a zone, as in the JDK. {@link #ofInstant(Instant, ZoneId)} and {@link
 * #ofEpochSecond} are the way from the wall clock to local fields; {@link #toEpochSecond} and
 * {@link #toInstant} the way back.
 */
@SuppressWarnings("ComparableType")
public final class LocalDateTime implements ChronoLocalDateTime<LocalDate> {
  private final LocalDate date;
  private final LocalTime time;

  private LocalDateTime(LocalDate date, LocalTime time) {
    this.date = date;
    this.time = time;
  }

  /** The current date-time in the default zone ({@code ZoneId.systemDefault()}). */
  public static LocalDateTime now() {
    return now(ZoneId.systemDefault());
  }

  public static LocalDateTime now(ZoneId zone) {
    return ofInstant(Instant.now(), zone);
  }

  public static LocalDateTime of(int year, Month month, int dayOfMonth, int hour, int minute) {
    return new LocalDateTime(LocalDate.of(year, month, dayOfMonth), LocalTime.of(hour, minute));
  }

  public static LocalDateTime of(
      int year, Month month, int dayOfMonth, int hour, int minute, int second) {
    return new LocalDateTime(
        LocalDate.of(year, month, dayOfMonth), LocalTime.of(hour, minute, second));
  }

  public static LocalDateTime of(int year, int month, int dayOfMonth, int hour, int minute) {
    return new LocalDateTime(LocalDate.of(year, month, dayOfMonth), LocalTime.of(hour, minute));
  }

  public static LocalDateTime of(
      int year, int month, int dayOfMonth, int hour, int minute, int second) {
    return new LocalDateTime(
        LocalDate.of(year, month, dayOfMonth), LocalTime.of(hour, minute, second));
  }

  public static LocalDateTime of(
      int year, int month, int dayOfMonth, int hour, int minute, int second, int nanoOfSecond) {
    return new LocalDateTime(
        LocalDate.of(year, month, dayOfMonth), LocalTime.of(hour, minute, second, nanoOfSecond));
  }

  public static LocalDateTime of(LocalDate date, LocalTime time) {
    if (date == null || time == null) {
      throw new NullPointerException("date and time");
    }
    return new LocalDateTime(date, time);
  }

  /** The local date-time of {@code instant} in {@code zone} (a fixed offset here). */
  public static LocalDateTime ofInstant(Instant instant, ZoneId zone) {
    return ofEpochSecond(
        instant.getEpochSecond(), instant.getNano(), zone.getRules().getOffset(instant));
  }

  public static LocalDateTime ofEpochSecond(long epochSecond, int nanoOfSecond, ZoneOffset offset) {
    if (nanoOfSecond < 0 || nanoOfSecond > 999_999_999) {
      throw new DateTimeException("Invalid value for NanoOfSecond: " + nanoOfSecond);
    }
    long localSecond = epochSecond + offset.getTotalSeconds();
    long localEpochDay = Math.floorDiv(localSecond, (long) LocalTime.SECONDS_PER_DAY);
    int secsOfDay = (int) Math.floorMod(localSecond, (long) LocalTime.SECONDS_PER_DAY);
    LocalDate date = LocalDate.ofEpochDay(localEpochDay);
    LocalTime time = LocalTime.ofNanoOfDay(secsOfDay * LocalTime.NANOS_PER_SECOND + nanoOfSecond);
    return new LocalDateTime(date, time);
  }

  /** Parses ISO-8601 {@code yyyy-MM-ddTHH:mm[:ss[.SSS]]}. */
  public static LocalDateTime parse(CharSequence text) {
    String s = text.toString();
    int t = s.indexOf('T');
    if (t < 0) {
      throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalDateTime");
    }
    return new LocalDateTime(
        LocalDate.parse(s.substring(0, t)), LocalTime.parse(s.substring(t + 1)));
  }

  static LocalDateTime from(Temporal temporal) {
    if (temporal instanceof LocalDateTime) {
      return (LocalDateTime) temporal;
    }
    if (temporal instanceof LocalDate) {
      return ((LocalDate) temporal).atStartOfDay();
    }
    throw new DateTimeException("Unable to obtain LocalDateTime from " + temporal);
  }

  private LocalDateTime with(LocalDate newDate, LocalTime newTime) {
    if (date.equals(newDate) && time.equals(newTime)) {
      return this;
    }
    return new LocalDateTime(newDate, newTime);
  }

  @Override
  public LocalDate toLocalDate() {
    return date;
  }

  @Override
  public LocalTime toLocalTime() {
    return time;
  }

  public int getYear() {
    return date.getYear();
  }

  public int getMonthValue() {
    return date.getMonthValue();
  }

  public Month getMonth() {
    return date.getMonth();
  }

  public int getDayOfMonth() {
    return date.getDayOfMonth();
  }

  public int getDayOfYear() {
    return date.getDayOfYear();
  }

  public DayOfWeek getDayOfWeek() {
    return date.getDayOfWeek();
  }

  public int getHour() {
    return time.getHour();
  }

  public int getMinute() {
    return time.getMinute();
  }

  public int getSecond() {
    return time.getSecond();
  }

  public int getNano() {
    return time.getNano();
  }

  @Override
  public boolean isSupported(TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      return unit != ChronoUnit.FOREVER;
    }
    return unit != null && unit.isSupportedBy(this);
  }

  public LocalDateTime withYear(int year) {
    return with(date.withYear(year), time);
  }

  public LocalDateTime withMonth(int month) {
    return with(date.withMonth(month), time);
  }

  public LocalDateTime withDayOfMonth(int dayOfMonth) {
    return with(date.withDayOfMonth(dayOfMonth), time);
  }

  public LocalDateTime withHour(int hour) {
    return with(date, time.withHour(hour));
  }

  public LocalDateTime withMinute(int minute) {
    return with(date, time.withMinute(minute));
  }

  public LocalDateTime withSecond(int second) {
    return with(date, time.withSecond(second));
  }

  public LocalDateTime withNano(int nanoOfSecond) {
    return with(date, time.withNano(nanoOfSecond));
  }

  public LocalDateTime truncatedTo(TemporalUnit unit) {
    return with(date, time.truncatedTo(unit));
  }

  @Override
  public LocalDateTime plus(long amountToAdd, TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      switch ((ChronoUnit) unit) {
        case NANOS:
          return plusNanos(amountToAdd);
        case MICROS:
          return plusDays(amountToAdd / 86_400_000_000L)
              .plusNanos((amountToAdd % 86_400_000_000L) * 1000);
        case MILLIS:
          return plusDays(amountToAdd / LocalTime.MILLIS_PER_DAY)
              .plusNanos((amountToAdd % LocalTime.MILLIS_PER_DAY) * 1_000_000);
        case SECONDS:
          return plusSeconds(amountToAdd);
        case MINUTES:
          return plusMinutes(amountToAdd);
        case HOURS:
          return plusHours(amountToAdd);
        case HALF_DAYS:
          return plusDays(amountToAdd / 256).plusHours((amountToAdd % 256) * 12);
        default:
          return with(date.plus(amountToAdd, unit), time);
      }
    }
    return unit.addTo(this, amountToAdd);
  }

  @Override
  public LocalDateTime minus(long amountToSubtract, TemporalUnit unit) {
    return amountToSubtract == Long.MIN_VALUE
        ? plus(Long.MAX_VALUE, unit).plus(1, unit)
        : plus(-amountToSubtract, unit);
  }

  /** As the JDK: {@code dateTime.plus(Duration.ofHours(2))}. */
  @Override
  public LocalDateTime plus(TemporalAmount amount) {
    return (LocalDateTime) amount.addTo(this);
  }

  @Override
  public LocalDateTime minus(TemporalAmount amount) {
    return (LocalDateTime) amount.subtractFrom(this);
  }

  public LocalDateTime plusYears(long years) {
    return with(date.plusYears(years), time);
  }

  public LocalDateTime plusMonths(long months) {
    return with(date.plusMonths(months), time);
  }

  public LocalDateTime plusWeeks(long weeks) {
    return with(date.plusWeeks(weeks), time);
  }

  public LocalDateTime plusDays(long days) {
    return with(date.plusDays(days), time);
  }

  public LocalDateTime plusHours(long hours) {
    return plusWithOverflow(date, hours, 0, 0, 0, 1);
  }

  public LocalDateTime plusMinutes(long minutes) {
    return plusWithOverflow(date, 0, minutes, 0, 0, 1);
  }

  public LocalDateTime plusSeconds(long seconds) {
    return plusWithOverflow(date, 0, 0, seconds, 0, 1);
  }

  public LocalDateTime plusNanos(long nanos) {
    return plusWithOverflow(date, 0, 0, 0, nanos, 1);
  }

  public LocalDateTime minusYears(long years) {
    return years == Long.MIN_VALUE ? plusYears(Long.MAX_VALUE).plusYears(1) : plusYears(-years);
  }

  public LocalDateTime minusMonths(long months) {
    return months == Long.MIN_VALUE
        ? plusMonths(Long.MAX_VALUE).plusMonths(1)
        : plusMonths(-months);
  }

  public LocalDateTime minusWeeks(long weeks) {
    return weeks == Long.MIN_VALUE ? plusWeeks(Long.MAX_VALUE).plusWeeks(1) : plusWeeks(-weeks);
  }

  public LocalDateTime minusDays(long days) {
    return days == Long.MIN_VALUE ? plusDays(Long.MAX_VALUE).plusDays(1) : plusDays(-days);
  }

  public LocalDateTime minusHours(long hours) {
    return plusWithOverflow(date, hours, 0, 0, 0, -1);
  }

  public LocalDateTime minusMinutes(long minutes) {
    return plusWithOverflow(date, 0, minutes, 0, 0, -1);
  }

  public LocalDateTime minusSeconds(long seconds) {
    return plusWithOverflow(date, 0, 0, seconds, 0, -1);
  }

  public LocalDateTime minusNanos(long nanos) {
    return plusWithOverflow(date, 0, 0, 0, nanos, -1);
  }

  private LocalDateTime plusWithOverflow(
      LocalDate newDate, long hours, long minutes, long seconds, long nanos, int sign) {
    if ((hours | minutes | seconds | nanos) == 0) {
      return with(newDate, time);
    }
    long totDays =
        nanos / LocalTime.NANOS_PER_DAY
            + seconds / LocalTime.SECONDS_PER_DAY
            + minutes / LocalTime.MINUTES_PER_DAY
            + hours / LocalTime.HOURS_PER_DAY;
    totDays *= sign;
    long totNanos =
        nanos % LocalTime.NANOS_PER_DAY
            + (seconds % LocalTime.SECONDS_PER_DAY) * LocalTime.NANOS_PER_SECOND
            + (minutes % LocalTime.MINUTES_PER_DAY) * LocalTime.NANOS_PER_MINUTE
            + (hours % LocalTime.HOURS_PER_DAY) * LocalTime.NANOS_PER_HOUR;
    long curNoD = time.toNanoOfDay();
    totNanos = totNanos * sign + curNoD;
    totDays += Math.floorDiv(totNanos, LocalTime.NANOS_PER_DAY);
    long newNoD = Math.floorMod(totNanos, LocalTime.NANOS_PER_DAY);
    LocalTime newTime = (newNoD == curNoD ? time : LocalTime.ofNanoOfDay(newNoD));
    return with(newDate.plusDays(totDays), newTime);
  }

  @Override
  public long until(Temporal endExclusive, TemporalUnit unit) {
    LocalDateTime end = from(endExclusive);
    if (unit instanceof ChronoUnit) {
      if (unit.isTimeBased()) {
        long amount = date.daysUntil(end.date);
        if (amount == 0) {
          return time.until(end.time, unit);
        }
        long timePart = end.time.toNanoOfDay() - time.toNanoOfDay();
        if (amount > 0) {
          amount--;
          timePart += LocalTime.NANOS_PER_DAY;
        } else {
          amount++;
          timePart -= LocalTime.NANOS_PER_DAY;
        }
        switch ((ChronoUnit) unit) {
          case NANOS:
            amount = Math.multiplyExact(amount, LocalTime.NANOS_PER_DAY);
            break;
          case MICROS:
            amount = Math.multiplyExact(amount, 86_400_000_000L);
            timePart = timePart / 1000;
            break;
          case MILLIS:
            amount = Math.multiplyExact(amount, LocalTime.MILLIS_PER_DAY);
            timePart = timePart / 1_000_000;
            break;
          case SECONDS:
            amount = Math.multiplyExact(amount, (long) LocalTime.SECONDS_PER_DAY);
            timePart = timePart / LocalTime.NANOS_PER_SECOND;
            break;
          case MINUTES:
            amount = Math.multiplyExact(amount, (long) LocalTime.MINUTES_PER_DAY);
            timePart = timePart / LocalTime.NANOS_PER_MINUTE;
            break;
          case HOURS:
            amount = Math.multiplyExact(amount, (long) LocalTime.HOURS_PER_DAY);
            timePart = timePart / LocalTime.NANOS_PER_HOUR;
            break;
          case HALF_DAYS:
            amount = Math.multiplyExact(amount, 2L);
            timePart = timePart / (LocalTime.NANOS_PER_HOUR * 12);
            break;
          default:
            throw new DateTimeException("Unsupported unit: " + unit);
        }
        return Math.addExact(amount, timePart);
      }
      LocalDate endDate = end.date;
      if (endDate.isAfter(date) && end.time.isBefore(time)) {
        endDate = endDate.minusDays(1);
      } else if (endDate.isBefore(date) && end.time.isAfter(time)) {
        endDate = endDate.plusDays(1);
      }
      return date.until(endDate, unit);
    }
    return unit.between(this, end);
  }

  public String format(DateTimeFormatter formatter) {
    return formatter.format(this);
  }

  // Declared here as well as inherited: the app-side API contract resolves a member on the
  // class and its superclasses, not through an interface's default, and every caller compiles
  // against LocalDateTime.

  @Override
  public long toEpochSecond(ZoneOffset offset) {
    long epochDay = date.toEpochDay();
    long secs = epochDay * 86400 + time.toSecondOfDay();
    return secs - offset.getTotalSeconds();
  }

  @Override
  public Instant toInstant(ZoneOffset offset) {
    return Instant.ofEpochSecond(toEpochSecond(offset), time.getNano());
  }

  @Override
  public int compareTo(ChronoLocalDateTime<?> other) {
    if (other instanceof LocalDateTime) {
      LocalDateTime o = (LocalDateTime) other;
      int cmp = date.compareTo(o.date);
      if (cmp == 0) {
        cmp = time.compareTo(o.time);
      }
      return cmp;
    }
    int cmp = date.compareTo(other.toLocalDate());
    if (cmp == 0) {
      cmp = time.compareTo(other.toLocalTime());
    }
    return cmp;
  }

  @Override
  public boolean isAfter(ChronoLocalDateTime<?> other) {
    return compareTo(other) > 0;
  }

  @Override
  public boolean isBefore(ChronoLocalDateTime<?> other) {
    return compareTo(other) < 0;
  }

  @Override
  public boolean isEqual(ChronoLocalDateTime<?> other) {
    return compareTo(other) == 0;
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (obj instanceof LocalDateTime) {
      LocalDateTime o = (LocalDateTime) obj;
      return date.equals(o.date) && time.equals(o.time);
    }
    return false;
  }

  @Override
  public int hashCode() {
    return date.hashCode() ^ time.hashCode();
  }

  /** ISO-8601, as the JDK prints it: {@code 2026-09-23T14:05:09}. */
  @Override
  public String toString() {
    StringBuilder buf = new StringBuilder(29);
    date.appendTo(buf);
    buf.append('T');
    time.appendTo(buf);
    return buf.toString();
  }
}
