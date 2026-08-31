// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * Time granularities, mirroring the commonly used part of {@code java.util.concurrent.TimeUnit}.
 */
public enum TimeUnit {
  NANOSECONDS(1L),
  MICROSECONDS(1000L),
  MILLISECONDS(1000000L),
  SECONDS(1000000000L),
  MINUTES(60000000000L),
  HOURS(3600000000000L),
  DAYS(86400000000000L);

  private final long nanos;

  TimeUnit(long nanos) {
    this.nanos = nanos;
  }

  /** {@code sourceDuration} in {@code sourceUnit}, expressed in this unit (truncating). */
  public long convert(long sourceDuration, TimeUnit sourceUnit) {
    if (sourceUnit.nanos >= nanos) {
      return sourceDuration * (sourceUnit.nanos / nanos);
    }
    return sourceDuration / (nanos / sourceUnit.nanos);
  }

  public long toNanos(long d) {
    return NANOSECONDS.convert(d, this);
  }

  public long toMicros(long d) {
    return MICROSECONDS.convert(d, this);
  }

  public long toMillis(long d) {
    return MILLISECONDS.convert(d, this);
  }

  public long toSeconds(long d) {
    return SECONDS.convert(d, this);
  }

  public long toMinutes(long d) {
    return MINUTES.convert(d, this);
  }

  /** {@link Thread#sleep(long)} for a duration in this unit. */
  public void sleep(long timeout) throws InterruptedException {
    if (timeout > 0) {
      Thread.sleep(toMillis(timeout));
    }
  }
}
