// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.temporal.ChronoUnit;
import java.time.temporal.Temporal;
import java.time.temporal.TemporalAmount;
import java.time.temporal.TemporalUnit;

/**
 * A point on the UTC time-line, as in the JDK. {@link #now()} reads {@code
 * System.currentTimeMillis()}, which counts from boot until something sets the wall clock ({@code
 * SystemClock.setCurrentTimeMillis}). {@code atZone} / {@code atOffset} are not served (no {@code
 * ZonedDateTime}); use {@link LocalDateTime#ofInstant(Instant, ZoneId)}.
 */
public final class Instant implements Temporal, Comparable<Instant> {
  public static final Instant EPOCH = new Instant(0, 0);

  private final long seconds;
  private final int nanos;

  private Instant(long epochSecond, int nanos) {
    this.seconds = epochSecond;
    this.nanos = nanos;
  }

  private static Instant create(long seconds, int nanoOfSecond) {
    if ((seconds | nanoOfSecond) == 0) {
      return EPOCH;
    }
    return new Instant(seconds, nanoOfSecond);
  }

  public static Instant now() {
    return ofEpochMilli(System.currentTimeMillis());
  }

  public static Instant ofEpochSecond(long epochSecond) {
    return create(epochSecond, 0);
  }

  public static Instant ofEpochSecond(long epochSecond, long nanoAdjustment) {
    long secs =
        Math.addExact(epochSecond, Math.floorDiv(nanoAdjustment, Duration.NANOS_PER_SECOND));
    int nos = (int) Math.floorMod(nanoAdjustment, Duration.NANOS_PER_SECOND);
    return create(secs, nos);
  }

  public static Instant ofEpochMilli(long epochMilli) {
    long secs = Math.floorDiv(epochMilli, 1000L);
    int mos = (int) Math.floorMod(epochMilli, 1000L);
    return create(secs, mos * 1_000_000);
  }

  public long getEpochSecond() {
    return seconds;
  }

  public int getNano() {
    return nanos;
  }

  public long toEpochMilli() {
    if (seconds < 0 && nanos > 0) {
      long millis = Math.multiplyExact(seconds + 1, 1000L);
      long adjustment = nanos / 1_000_000 - 1000;
      return Math.addExact(millis, adjustment);
    }
    long millis = Math.multiplyExact(seconds, 1000L);
    return Math.addExact(millis, nanos / 1_000_000);
  }

  @Override
  public boolean isSupported(TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      return unit.isTimeBased() || unit == ChronoUnit.DAYS;
    }
    return unit != null && unit.isSupportedBy(this);
  }

  /** As the JDK: {@code instant.plus(Duration.ofMinutes(5))}. */
  @Override
  public Instant plus(TemporalAmount amount) {
    return (Instant) amount.addTo(this);
  }

  @Override
  public Instant plus(long amountToAdd, TemporalUnit unit) {
    if (unit instanceof ChronoUnit) {
      switch ((ChronoUnit) unit) {
        case NANOS:
          return plusNanos(amountToAdd);
        case MICROS:
          return plus(amountToAdd / 1_000_000, (amountToAdd % 1_000_000) * 1000);
        case MILLIS:
          return plusMillis(amountToAdd);
        case SECONDS:
          return plusSeconds(amountToAdd);
        case MINUTES:
          return plusSeconds(Math.multiplyExact(amountToAdd, 60L));
        case HOURS:
          return plusSeconds(Math.multiplyExact(amountToAdd, 3600L));
        case HALF_DAYS:
          return plusSeconds(Math.multiplyExact(amountToAdd, 43200L));
        case DAYS:
          return plusSeconds(Math.multiplyExact(amountToAdd, Duration.SECONDS_PER_DAY));
        default:
          throw new DateTimeException("Unsupported unit: " + unit);
      }
    }
    return unit.addTo(this, amountToAdd);
  }

  public Instant plusSeconds(long secondsToAdd) {
    return plus(secondsToAdd, 0);
  }

  public Instant plusMillis(long millisToAdd) {
    return plus(millisToAdd / 1000, (millisToAdd % 1000) * 1_000_000);
  }

  public Instant plusNanos(long nanosToAdd) {
    return plus(0, nanosToAdd);
  }

  private Instant plus(long secondsToAdd, long nanosToAdd) {
    if ((secondsToAdd | nanosToAdd) == 0) {
      return this;
    }
    long epochSec = Math.addExact(seconds, secondsToAdd);
    epochSec = Math.addExact(epochSec, nanosToAdd / Duration.NANOS_PER_SECOND);
    nanosToAdd = nanosToAdd % Duration.NANOS_PER_SECOND;
    long nanoAdjustment = nanos + nanosToAdd;
    return ofEpochSecond(epochSec, nanoAdjustment);
  }

  @Override
  public Instant minus(TemporalAmount amount) {
    return (Instant) amount.subtractFrom(this);
  }

  @Override
  public Instant minus(long amountToSubtract, TemporalUnit unit) {
    return amountToSubtract == Long.MIN_VALUE
        ? plus(Long.MAX_VALUE, unit).plus(1, unit)
        : plus(-amountToSubtract, unit);
  }

  public Instant minusSeconds(long secondsToSubtract) {
    return plusSeconds(-secondsToSubtract);
  }

  public Instant minusMillis(long millisToSubtract) {
    return plusMillis(-millisToSubtract);
  }

  public Instant minusNanos(long nanosToSubtract) {
    return plusNanos(-nanosToSubtract);
  }

  static Instant from(Temporal temporal) {
    if (temporal instanceof Instant) {
      return (Instant) temporal;
    }
    throw new DateTimeException("Unable to obtain Instant from " + temporal);
  }

  @Override
  public long until(Temporal endExclusive, TemporalUnit unit) {
    Instant end = from(endExclusive);
    if (unit instanceof ChronoUnit) {
      switch ((ChronoUnit) unit) {
        case NANOS:
          return nanosUntil(end);
        case MICROS:
          return nanosUntil(end) / 1000;
        case MILLIS:
          return Math.subtractExact(end.toEpochMilli(), toEpochMilli());
        case SECONDS:
          return secondsUntil(end);
        case MINUTES:
          return secondsUntil(end) / 60;
        case HOURS:
          return secondsUntil(end) / 3600;
        case HALF_DAYS:
          return secondsUntil(end) / 43200;
        case DAYS:
          return secondsUntil(end) / Duration.SECONDS_PER_DAY;
        default:
          throw new DateTimeException("Unsupported unit: " + unit);
      }
    }
    return unit.between(this, end);
  }

  private long nanosUntil(Instant end) {
    long secsDiff = Math.subtractExact(end.seconds, seconds);
    long totalNanos = Math.multiplyExact(secsDiff, Duration.NANOS_PER_SECOND);
    return Math.addExact(totalNanos, end.nanos - nanos);
  }

  private long secondsUntil(Instant end) {
    long secsDiff = Math.subtractExact(end.seconds, seconds);
    long nanosDiff = (long) end.nanos - nanos;
    if (secsDiff > 0 && nanosDiff < 0) {
      secsDiff--;
    } else if (secsDiff < 0 && nanosDiff > 0) {
      secsDiff++;
    }
    return secsDiff;
  }

  public boolean isAfter(Instant otherInstant) {
    return compareTo(otherInstant) > 0;
  }

  public boolean isBefore(Instant otherInstant) {
    return compareTo(otherInstant) < 0;
  }

  @Override
  public int compareTo(Instant otherInstant) {
    if (seconds != otherInstant.seconds) {
      return seconds < otherInstant.seconds ? -1 : 1;
    }
    return nanos - otherInstant.nanos;
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (obj instanceof Instant) {
      Instant other = (Instant) obj;
      return seconds == other.seconds && nanos == other.nanos;
    }
    return false;
  }

  @Override
  public int hashCode() {
    return ((int) (seconds ^ (seconds >>> 32))) + 51 * nanos;
  }

  /** ISO-8601 in UTC, as the JDK prints it: {@code 2026-09-23T14:05:09Z}. */
  @Override
  public String toString() {
    return LocalDateTime.ofEpochSecond(seconds, nanos, ZoneOffset.UTC).toString() + "Z";
  }
}
