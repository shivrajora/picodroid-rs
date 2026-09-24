// SPDX-License-Identifier: GPL-3.0-only
package java.time.chrono;

import java.time.Instant;
import java.time.LocalTime;
import java.time.ZoneOffset;
import java.time.temporal.Temporal;

/**
 * A date-time without a zone. {@code LocalDateTime} is the only implementation; this interface
 * exists for the JDK descriptors an app's calls carry ({@code isBefore(ChronoLocalDateTime)},
 * {@code compareTo(ChronoLocalDateTime)}).
 */
public interface ChronoLocalDateTime<D extends ChronoLocalDate>
    extends Temporal, Comparable<ChronoLocalDateTime<?>> {
  D toLocalDate();

  LocalTime toLocalTime();

  default long toEpochSecond(ZoneOffset offset) {
    long epochDay = toLocalDate().toEpochDay();
    long secs = epochDay * 86400 + toLocalTime().toSecondOfDay();
    return secs - offset.getTotalSeconds();
  }

  default Instant toInstant(ZoneOffset offset) {
    return Instant.ofEpochSecond(toEpochSecond(offset), toLocalTime().getNano());
  }

  default boolean isAfter(ChronoLocalDateTime<?> other) {
    return compareTo(other) > 0;
  }

  default boolean isBefore(ChronoLocalDateTime<?> other) {
    return compareTo(other) < 0;
  }

  default boolean isEqual(ChronoLocalDateTime<?> other) {
    return compareTo(other) == 0;
  }

  @Override
  default int compareTo(ChronoLocalDateTime<?> other) {
    int cmp = toLocalDate().compareTo(other.toLocalDate());
    if (cmp == 0) {
      cmp = toLocalTime().compareTo(other.toLocalTime());
    }
    return cmp;
  }
}
