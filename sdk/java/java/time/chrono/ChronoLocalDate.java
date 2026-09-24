// SPDX-License-Identifier: GPL-3.0-only
package java.time.chrono;

import java.time.LocalTime;
import java.time.temporal.Temporal;

/**
 * A date without a time or zone. {@code LocalDate} is the only implementation (there is no other
 * chronology), so this interface exists for the JDK descriptors an app's {@code LocalDate} calls
 * carry: {@code isBefore(ChronoLocalDate)}, {@code compareTo(ChronoLocalDate)} and so on.
 */
public interface ChronoLocalDate extends Temporal, Comparable<ChronoLocalDate> {
  /** Days since 1970-01-01. */
  long toEpochDay();

  int lengthOfMonth();

  default int lengthOfYear() {
    return isLeapYear() ? 366 : 365;
  }

  boolean isLeapYear();

  ChronoLocalDateTime<?> atTime(LocalTime localTime);

  default boolean isAfter(ChronoLocalDate other) {
    return toEpochDay() > other.toEpochDay();
  }

  default boolean isBefore(ChronoLocalDate other) {
    return toEpochDay() < other.toEpochDay();
  }

  default boolean isEqual(ChronoLocalDate other) {
    return toEpochDay() == other.toEpochDay();
  }

  @Override
  default int compareTo(ChronoLocalDate other) {
    long a = toEpochDay();
    long b = other.toEpochDay();
    return a < b ? -1 : (a > b ? 1 : 0);
  }
}
