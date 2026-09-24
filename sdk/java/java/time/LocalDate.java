// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.chrono.ChronoLocalDate;
import java.time.format.DateTimeFormatter;
import java.time.temporal.ChronoUnit;
import java.time.temporal.Temporal;
import java.time.temporal.TemporalAmount;
import java.time.temporal.TemporalUnit;

/**
 * A date without a time or zone in the ISO calendar, as in the JDK. Month arithmetic clamps the
 * day-of-month to the target month's length ({@code 2026-01-31 plusMonths(1)} is {@code
 * 2026-02-28}), and {@link #until} counts complete units, both as the JDK does.
 */
@SuppressWarnings("ComparableType")
public final class LocalDate implements ChronoLocalDate {
  public static final LocalDate MIN = new LocalDate(Year.MIN_VALUE, 1, 1);
  public static final LocalDate MAX = new LocalDate(Year.MAX_VALUE, 12, 31);
  public static final LocalDate EPOCH = new LocalDate(1970, 1, 1);

  private static final int DAYS_PER_CYCLE = 146097;
  private static final long DAYS_0000_TO_1970 = (DAYS_PER_CYCLE * 5L) - (30L * 365L + 7L);

  private final int year;
  private final int month;
  private final int day;

  private LocalDate(int year, int month, int dayOfMonth) {
    this.year = year;
    this.month = month;
    this.day = dayOfMonth;
  }

  /** Today in the default zone ({@code ZoneId.systemDefault()}). */
  public static LocalDate now() {
    return now(ZoneId.systemDefault());
  }

  public static LocalDate now(ZoneId zone) {
    return LocalDateTime.now(zone).toLocalDate();
  }

  public static LocalDate of(int year, Month month, int dayOfMonth) {
    return of(year, month.getValue(), dayOfMonth);
  }

  public static LocalDate of(int year, int month, int dayOfMonth) {
    checkYear(year);
    if (month < 1 || month > 12) {
      throw new DateTimeException("Invalid value for MonthOfYear: " + month);
    }
    if (dayOfMonth < 1 || dayOfMonth > 31) {
      throw new DateTimeException("Invalid value for DayOfMonth: " + dayOfMonth);
    }
    if (dayOfMonth > 28 && dayOfMonth > Month.of(month).length(Year.isLeap(year))) {
      if (dayOfMonth == 29) {
        throw new DateTimeException(
            "Invalid date 'February 29' as '" + year + "' is not a leap year");
      }
      throw new DateTimeException("Invalid date '" + Month.of(month) + " " + dayOfMonth + "'");
    }
    return new LocalDate(year, month, dayOfMonth);
  }

  public static LocalDate ofYearDay(int year, int dayOfYear) {
    checkYear(year);
    boolean leap = Year.isLeap(year);
    if (dayOfYear < 1 || dayOfYear > (leap ? 366 : 365)) {
      throw new DateTimeException("Invalid date 'DayOfYear " + dayOfYear + "' for year " + year);
    }
    int m = 1;
    while (m < 12 && Month.of(m + 1).firstDayOfYear(leap) <= dayOfYear) {
      m++;
    }
    int dom = dayOfYear - Month.of(m).firstDayOfYear(leap) + 1;
    return new LocalDate(year, m, dom);
  }

  /** Days since 1970-01-01, negative before it. */
  public static LocalDate ofEpochDay(long epochDay) {
    long zeroDay = epochDay + DAYS_0000_TO_1970;
    // Shift so that the cycle starts on March 1st; the JDK's own algorithm.
    zeroDay -= 60;
    long adjust = 0;
    if (zeroDay < 0) {
      long adjustCycles = (zeroDay + 1) / DAYS_PER_CYCLE - 1;
      adjust = adjustCycles * 400;
      zeroDay += -adjustCycles * DAYS_PER_CYCLE;
    }
    long yearEst = (400 * zeroDay + 591) / DAYS_PER_CYCLE;
    long doyEst = zeroDay - (365 * yearEst + yearEst / 4 - yearEst / 100 + yearEst / 400);
    if (doyEst < 0) {
      yearEst--;
      doyEst = zeroDay - (365 * yearEst + yearEst / 4 - yearEst / 100 + yearEst / 400);
    }
    yearEst += adjust;
    int marchDoy0 = (int) doyEst;
    int marchMonth0 = (marchDoy0 * 5 + 2) / 153;
    int month = (marchMonth0 + 2) % 12 + 1;
    int dom = marchDoy0 - (marchMonth0 * 306 + 5) / 10 + 1;
    yearEst += marchMonth0 / 10;
    checkYear(yearEst);
    return new LocalDate((int) yearEst, month, dom);
  }

  /** Parses ISO-8601 {@code yyyy-MM-dd}. */
  public static LocalDate parse(CharSequence text) {
    String s = text.toString();
    try {
      int len = s.length();
      int dash1 = s.indexOf('-', 1);
      if (dash1 < 4 || len != dash1 + 6 || s.charAt(dash1 + 3) != '-') {
        throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalDate");
      }
      int year = Integer.parseInt(s.substring(0, dash1));
      int month = Integer.parseInt(s.substring(dash1 + 1, dash1 + 3));
      int day = Integer.parseInt(s.substring(dash1 + 4));
      return of(year, month, day);
    } catch (NumberFormatException e) {
      throw new DateTimeException("Text '" + s + "' could not be parsed as a LocalDate");
    }
  }

  private static void checkYear(long year) {
    if (year < Year.MIN_VALUE || year > Year.MAX_VALUE) {
      throw new DateTimeException("Invalid value for Year: " + year);
    }
  }

  static LocalDate from(Temporal temporal) {
    if (temporal instanceof LocalDate) {
      return (LocalDate) temporal;
    }
    if (temporal instanceof LocalDateTime) {
      return ((LocalDateTime) temporal).toLocalDate();
    }
    throw new DateTimeException("Unable to obtain LocalDate from " + temporal);
  }

  public int getYear() {
    return year;
  }

  public int getMonthValue() {
    return month;
  }

  public Month getMonth() {
    return Month.of(month);
  }

  public int getDayOfMonth() {
    return day;
  }

  public int getDayOfYear() {
    return getMonth().firstDayOfYear(isLeapYear()) + day - 1;
  }

  public DayOfWeek getDayOfWeek() {
    int dow0 = (int) Math.floorMod(toEpochDay() + 3, 7L);
    return DayOfWeek.of(dow0 + 1);
  }

  @Override
  public boolean isLeapYear() {
    return Year.isLeap(year);
  }

  @Override
  public int lengthOfMonth() {
    return getMonth().length(isLeapYear());
  }

  @Override
  public int lengthOfYear() {
    return isLeapYear() ? 366 : 365;
  }

  @Override
  public boolean isSupported(TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      return unit.isDateBased();
    }
    return unit != null && unit.isSupportedBy(this);
  }

  public LocalDate withYear(int year) {
    return this.year == year ? this : resolvePreviousValid(year, month, day);
  }

  public LocalDate withMonth(int month) {
    return this.month == month ? this : resolvePreviousValid(year, month, day);
  }

  public LocalDate withDayOfMonth(int dayOfMonth) {
    return this.day == dayOfMonth ? this : of(year, month, dayOfMonth);
  }

  public LocalDate withDayOfYear(int dayOfYear) {
    return getDayOfYear() == dayOfYear ? this : ofYearDay(year, dayOfYear);
  }

  private static LocalDate resolvePreviousValid(int year, int month, int day) {
    if (month < 1 || month > 12) {
      throw new DateTimeException("Invalid value for MonthOfYear: " + month);
    }
    checkYear(year);
    int max = Month.of(month).length(Year.isLeap(year));
    return new LocalDate(year, month, day > max ? max : day);
  }

  @Override
  public LocalDate plus(TemporalAmount amount) {
    return (LocalDate) amount.addTo(this);
  }

  @Override
  public LocalDate minus(TemporalAmount amount) {
    return (LocalDate) amount.subtractFrom(this);
  }

  @Override
  public LocalDate plus(long amountToAdd, TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      switch ((ChronoUnit) unit) {
        case DAYS:
          return plusDays(amountToAdd);
        case WEEKS:
          return plusWeeks(amountToAdd);
        case MONTHS:
          return plusMonths(amountToAdd);
        case YEARS:
          return plusYears(amountToAdd);
        case DECADES:
          return plusYears(Math.multiplyExact(amountToAdd, 10L));
        case CENTURIES:
          return plusYears(Math.multiplyExact(amountToAdd, 100L));
        case MILLENNIA:
          return plusYears(Math.multiplyExact(amountToAdd, 1000L));
        default:
          throw new DateTimeException("Unsupported unit: " + unit);
      }
    }
    return unit.addTo(this, amountToAdd);
  }

  @Override
  public LocalDate minus(long amountToSubtract, TemporalUnit unit) {
    return amountToSubtract == Long.MIN_VALUE
        ? plus(Long.MAX_VALUE, unit).plus(1, unit)
        : plus(-amountToSubtract, unit);
  }

  public LocalDate plusYears(long yearsToAdd) {
    if (yearsToAdd == 0) {
      return this;
    }
    long newYear = year + yearsToAdd;
    checkYear(newYear);
    return resolvePreviousValid((int) newYear, month, day);
  }

  public LocalDate plusMonths(long monthsToAdd) {
    if (monthsToAdd == 0) {
      return this;
    }
    long monthCount = year * 12L + (month - 1);
    long calcMonths = monthCount + monthsToAdd;
    long newYear = Math.floorDiv(calcMonths, 12L);
    checkYear(newYear);
    int newMonth = (int) Math.floorMod(calcMonths, 12L) + 1;
    return resolvePreviousValid((int) newYear, newMonth, day);
  }

  public LocalDate plusWeeks(long weeksToAdd) {
    return plusDays(Math.multiplyExact(weeksToAdd, 7L));
  }

  public LocalDate plusDays(long daysToAdd) {
    if (daysToAdd == 0) {
      return this;
    }
    long dom = day + daysToAdd;
    if (dom > 0) {
      if (dom <= 28) {
        return new LocalDate(year, month, (int) dom);
      } else if (dom <= 59) {
        long monthLen = lengthOfMonth();
        if (dom <= monthLen) {
          return new LocalDate(year, month, (int) dom);
        } else if (month < 12) {
          return new LocalDate(year, month + 1, (int) (dom - monthLen));
        } else {
          checkYear(year + 1L);
          return new LocalDate(year + 1, 1, (int) (dom - monthLen));
        }
      }
    }
    long mjDay = Math.addExact(toEpochDay(), daysToAdd);
    return ofEpochDay(mjDay);
  }

  public LocalDate minusYears(long yearsToSubtract) {
    return yearsToSubtract == Long.MIN_VALUE
        ? plusYears(Long.MAX_VALUE).plusYears(1)
        : plusYears(-yearsToSubtract);
  }

  public LocalDate minusMonths(long monthsToSubtract) {
    return monthsToSubtract == Long.MIN_VALUE
        ? plusMonths(Long.MAX_VALUE).plusMonths(1)
        : plusMonths(-monthsToSubtract);
  }

  public LocalDate minusWeeks(long weeksToSubtract) {
    return weeksToSubtract == Long.MIN_VALUE
        ? plusWeeks(Long.MAX_VALUE).plusWeeks(1)
        : plusWeeks(-weeksToSubtract);
  }

  public LocalDate minusDays(long daysToSubtract) {
    return daysToSubtract == Long.MIN_VALUE
        ? plusDays(Long.MAX_VALUE).plusDays(1)
        : plusDays(-daysToSubtract);
  }

  @Override
  public long until(Temporal endExclusive, TemporalUnit unit) {
    LocalDate end = from(endExclusive);
    if (unit instanceof ChronoUnit) {
      switch ((ChronoUnit) unit) {
        case DAYS:
          return daysUntil(end);
        case WEEKS:
          return daysUntil(end) / 7;
        case MONTHS:
          return monthsUntil(end);
        case YEARS:
          return monthsUntil(end) / 12;
        case DECADES:
          return monthsUntil(end) / 120;
        case CENTURIES:
          return monthsUntil(end) / 1200;
        case MILLENNIA:
          return monthsUntil(end) / 12000;
        default:
          throw new DateTimeException("Unsupported unit: " + unit);
      }
    }
    return unit.between(this, end);
  }

  long daysUntil(LocalDate end) {
    return end.toEpochDay() - toEpochDay();
  }

  private long monthsUntil(LocalDate end) {
    long packed1 = prolepticMonth() * 32L + day;
    long packed2 = end.prolepticMonth() * 32L + end.day;
    return (packed2 - packed1) / 32;
  }

  private long prolepticMonth() {
    return year * 12L + month - 1;
  }

  public LocalDateTime atStartOfDay() {
    return LocalDateTime.of(this, LocalTime.MIDNIGHT);
  }

  @Override
  public LocalDateTime atTime(LocalTime time) {
    return LocalDateTime.of(this, time);
  }

  public LocalDateTime atTime(int hour, int minute) {
    return atTime(LocalTime.of(hour, minute));
  }

  public LocalDateTime atTime(int hour, int minute, int second) {
    return atTime(LocalTime.of(hour, minute, second));
  }

  @Override
  public long toEpochDay() {
    long y = year;
    long m = month;
    long total = 0;
    total += 365 * y;
    if (y >= 0) {
      total += (y + 3) / 4 - (y + 99) / 100 + (y + 399) / 400;
    } else {
      total -= y / -4 - y / -100 + y / -400;
    }
    total += ((367 * m - 362) / 12);
    total += day - 1;
    if (m > 2) {
      total--;
      if (!isLeapYear()) {
        total--;
      }
    }
    return total - DAYS_0000_TO_1970;
  }

  public String format(DateTimeFormatter formatter) {
    return formatter.format(this);
  }

  @Override
  public int compareTo(ChronoLocalDate other) {
    if (other instanceof LocalDate) {
      LocalDate o = (LocalDate) other;
      int cmp = year - o.year;
      if (cmp == 0) {
        cmp = month - o.month;
        if (cmp == 0) {
          cmp = day - o.day;
        }
      }
      return cmp;
    }
    long a = toEpochDay();
    long b = other.toEpochDay();
    return a < b ? -1 : (a > b ? 1 : 0);
  }

  @Override
  public boolean isAfter(ChronoLocalDate other) {
    return compareTo(other) > 0;
  }

  @Override
  public boolean isBefore(ChronoLocalDate other) {
    return compareTo(other) < 0;
  }

  @Override
  public boolean isEqual(ChronoLocalDate other) {
    return compareTo(other) == 0;
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (obj instanceof LocalDate) {
      LocalDate o = (LocalDate) obj;
      return year == o.year && month == o.month && day == o.day;
    }
    return false;
  }

  @Override
  public int hashCode() {
    return (year & 0xFFFFF800) ^ ((year << 11) + (month << 6) + day);
  }

  /** ISO-8601, as the JDK prints it: {@code 2026-09-23}. */
  @Override
  public String toString() {
    StringBuilder buf = new StringBuilder(10);
    appendTo(buf);
    return buf.toString();
  }

  void appendTo(StringBuilder buf) {
    int absYear = Math.abs(year);
    if (absYear < 1000) {
      if (year < 0) {
        buf.append('-');
      }
      buf.append(Integer.toString(absYear + 10000).substring(1));
    } else {
      if (year > 9999) {
        buf.append('+');
      }
      buf.append(year);
    }
    buf.append('-');
    LocalTime.twoDigits(buf, month);
    buf.append('-');
    LocalTime.twoDigits(buf, day);
  }
}
