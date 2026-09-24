// SPDX-License-Identifier: GPL-3.0-only
package java.time.temporal;

import java.time.Duration;

/** A unit of time, implemented by {@link ChronoUnit}. */
public interface TemporalUnit {
  Duration getDuration();

  boolean isDurationEstimated();

  boolean isDateBased();

  boolean isTimeBased();

  default boolean isSupportedBy(Temporal temporal) {
    return temporal.isSupported(this);
  }

  <R extends Temporal> R addTo(R temporal, long amount);

  long between(Temporal temporal1Inclusive, Temporal temporal2Exclusive);
}
