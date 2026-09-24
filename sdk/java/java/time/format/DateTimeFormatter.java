// SPDX-License-Identifier: GPL-3.0-only
package java.time.format;

import java.time.DateTimeException;
import java.time.DayOfWeek;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.LocalTime;
import java.time.Month;
import java.time.temporal.TemporalAccessor;

/**
 * Formats a {@link LocalDate}, {@link LocalTime} or {@link LocalDateTime} from a pattern, as the
 * JDK's {@code ofPattern} does for the common letters: {@code y u} year, {@code M L} month (number,
 * or {@code MMM} / {@code MMMM} for the English name), {@code d} day, {@code D} day-of-year, {@code
 * E} day-of-week name ({@code EEEE} in full), {@code a} AM/PM, {@code H} hour 0-23, {@code h} hour
 * 1-12, {@code m} minute, {@code s} second, {@code S} fraction, and {@code '} to quote literal
 * text. Letters repeated {@code n} times pad the number to {@code n} digits ({@code yy} prints the
 * two-digit year). Locale-sensitive letters and parsing are not served: {@code LocalDate.parse} and
 * friends read ISO-8601 only.
 */
public final class DateTimeFormatter {
  public static final DateTimeFormatter ISO_LOCAL_DATE = new DateTimeFormatter("yyyy-MM-dd");
  public static final DateTimeFormatter ISO_LOCAL_TIME = new DateTimeFormatter("HH:mm:ss");
  public static final DateTimeFormatter ISO_LOCAL_DATE_TIME =
      new DateTimeFormatter("yyyy-MM-dd'T'HH:mm:ss");

  private static final String[] MONTHS = {
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December"
  };
  private static final String[] DAYS = {
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"
  };

  private final String pattern;

  private DateTimeFormatter(String pattern) {
    this.pattern = pattern;
  }

  public static DateTimeFormatter ofPattern(String pattern) {
    if (pattern == null) {
      throw new NullPointerException("pattern");
    }
    // Reject unknown letters up front, as the JDK does, so a typo fails at construction.
    boolean quoted = false;
    for (int i = 0; i < pattern.length(); i++) {
      char c = pattern.charAt(i);
      if (c == '\'') {
        quoted = !quoted;
      } else if (!quoted && isLetter(c) && "yuMLdDEaHhmsS".indexOf(c) < 0) {
        throw new IllegalArgumentException("Unknown pattern letter: " + c);
      }
    }
    return new DateTimeFormatter(pattern);
  }

  private static boolean isLetter(char c) {
    return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');
  }

  public String format(TemporalAccessor temporal) {
    LocalDate date = null;
    LocalTime time = null;
    if (temporal instanceof LocalDateTime) {
      date = ((LocalDateTime) temporal).toLocalDate();
      time = ((LocalDateTime) temporal).toLocalTime();
    } else if (temporal instanceof LocalDate) {
      date = (LocalDate) temporal;
    } else if (temporal instanceof LocalTime) {
      time = (LocalTime) temporal;
    } else {
      throw new DateTimeException("Unable to format " + temporal);
    }
    StringBuilder out = new StringBuilder(pattern.length() + 8);
    int n = pattern.length();
    int i = 0;
    while (i < n) {
      char c = pattern.charAt(i);
      if (c == '\'') {
        int close = pattern.indexOf('\'', i + 1);
        if (close < 0) {
          throw new IllegalArgumentException("Pattern ends with an incomplete string literal");
        }
        if (close == i + 1) {
          out.append('\'');
        } else {
          out.append(pattern.substring(i + 1, close));
        }
        i = close + 1;
        continue;
      }
      if (!isLetter(c)) {
        out.append(c);
        i++;
        continue;
      }
      int count = 1;
      while (i + count < n && pattern.charAt(i + count) == c) {
        count++;
      }
      appendField(out, c, count, date, time);
      i += count;
    }
    return out.toString();
  }

  private static void appendField(
      StringBuilder out, char letter, int count, LocalDate date, LocalTime time) {
    switch (letter) {
      case 'y':
      case 'u':
        {
          int year = date(date, letter).getYear();
          if (count == 2) {
            pad(out, Math.floorMod(year, 100), 2);
          } else {
            pad(out, year, count);
          }
          return;
        }
      case 'M':
      case 'L':
        {
          Month m = date(date, letter).getMonth();
          if (count >= 4) {
            out.append(MONTHS[m.getValue() - 1]);
          } else if (count == 3) {
            out.append(MONTHS[m.getValue() - 1].substring(0, 3));
          } else {
            pad(out, m.getValue(), count);
          }
          return;
        }
      case 'd':
        pad(out, date(date, letter).getDayOfMonth(), count);
        return;
      case 'D':
        pad(out, date(date, letter).getDayOfYear(), count);
        return;
      case 'E':
        {
          DayOfWeek dow = date(date, letter).getDayOfWeek();
          String name = DAYS[dow.getValue() - 1];
          out.append(count >= 4 ? name : name.substring(0, 3));
          return;
        }
      case 'a':
        out.append(time(time, letter).getHour() < 12 ? "AM" : "PM");
        return;
      case 'H':
        pad(out, time(time, letter).getHour(), count);
        return;
      case 'h':
        {
          int h = time(time, letter).getHour() % 12;
          pad(out, h == 0 ? 12 : h, count);
          return;
        }
      case 'm':
        pad(out, time(time, letter).getMinute(), count);
        return;
      case 's':
        pad(out, time(time, letter).getSecond(), count);
        return;
      case 'S':
        {
          // The first `count` digits of the nanosecond fraction, truncated.
          int nano = time(time, letter).getNano();
          String digits = Integer.toString(nano + 1_000_000_000).substring(1);
          out.append(count >= 9 ? digits : digits.substring(0, count));
          return;
        }
      default:
        throw new IllegalArgumentException("Unknown pattern letter: " + letter);
    }
  }

  private static LocalDate date(LocalDate date, char letter) {
    if (date == null) {
      throw new DateTimeException("Unsupported field for a time: " + letter);
    }
    return date;
  }

  private static LocalTime time(LocalTime time, char letter) {
    if (time == null) {
      throw new DateTimeException("Unsupported field for a date: " + letter);
    }
    return time;
  }

  private static void pad(StringBuilder out, int value, int width) {
    if (value < 0) {
      out.append('-');
      value = -value;
    }
    String s = Integer.toString(value);
    for (int i = s.length(); i < width; i++) {
      out.append('0');
    }
    out.append(s);
  }

  @Override
  public String toString() {
    return pattern;
  }
}
