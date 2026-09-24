// SPDX-License-Identifier: GPL-3.0-only
package java.time.temporal;

import java.time.Duration;

/** The standard units, as in the JDK. */
public enum ChronoUnit implements TemporalUnit {
  NANOS(Duration.ofNanos(1)),
  MICROS(Duration.ofNanos(1000)),
  MILLIS(Duration.ofNanos(1_000_000)),
  SECONDS(Duration.ofSeconds(1)),
  MINUTES(Duration.ofSeconds(60)),
  HOURS(Duration.ofSeconds(3600)),
  HALF_DAYS(Duration.ofSeconds(43200)),
  DAYS(Duration.ofSeconds(86400)),
  WEEKS(Duration.ofSeconds(7 * 86400L)),
  MONTHS(Duration.ofSeconds(31556952L / 12)),
  YEARS(Duration.ofSeconds(31556952L)),
  DECADES(Duration.ofSeconds(31556952L * 10L)),
  CENTURIES(Duration.ofSeconds(31556952L * 100L)),
  MILLENNIA(Duration.ofSeconds(31556952L * 1000L)),
  ERAS(Duration.ofSeconds(31556952L * 1000_000_000L)),
  FOREVER(Duration.ofSeconds(Long.MAX_VALUE, 999_999_999));

  private final Duration duration;

  ChronoUnit(Duration estimatedDuration) {
    this.duration = estimatedDuration;
  }

  @Override
  public Duration getDuration() {
    return duration;
  }

  @Override
  public boolean isDurationEstimated() {
    return compareTo(DAYS) >= 0;
  }

  @Override
  public boolean isDateBased() {
    return compareTo(DAYS) >= 0 && this != FOREVER;
  }

  @Override
  public boolean isTimeBased() {
    return compareTo(DAYS) < 0;
  }

  @Override
  public boolean isSupportedBy(Temporal temporal) {
    return temporal.isSupported(this);
  }

  @SuppressWarnings("unchecked")
  @Override
  public <R extends Temporal> R addTo(R temporal, long amount) {
    return (R) temporal.plus(amount, this);
  }

  @Override
  public long between(Temporal temporal1Inclusive, Temporal temporal2Exclusive) {
    return temporal1Inclusive.until(temporal2Exclusive, this);
  }
}
