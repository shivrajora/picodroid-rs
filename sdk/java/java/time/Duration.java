// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.temporal.ChronoUnit;
import java.time.temporal.Temporal;
import java.time.temporal.TemporalAmount;
import java.time.temporal.TemporalUnit;
import java.util.ArrayList;
import java.util.List;

/**
 * A length of time in seconds and nanoseconds, as in the JDK. Arithmetic is exact and throws {@link
 * ArithmeticException} on overflow, like the JDK's.
 */
public final class Duration implements TemporalAmount, Comparable<Duration> {
  public static final Duration ZERO = new Duration(0, 0);

  static final long NANOS_PER_SECOND = 1_000_000_000L;
  static final long SECONDS_PER_DAY = 86400L;

  private final long seconds;
  private final int nanos;

  private Duration(long seconds, int nanos) {
    this.seconds = seconds;
    this.nanos = nanos;
  }

  private static Duration create(long seconds, int nanoAdjustment) {
    if ((seconds | nanoAdjustment) == 0) {
      return ZERO;
    }
    return new Duration(seconds, nanoAdjustment);
  }

  public static Duration ofDays(long days) {
    return create(Math.multiplyExact(days, SECONDS_PER_DAY), 0);
  }

  public static Duration ofHours(long hours) {
    return create(Math.multiplyExact(hours, 3600L), 0);
  }

  public static Duration ofMinutes(long minutes) {
    return create(Math.multiplyExact(minutes, 60L), 0);
  }

  public static Duration ofSeconds(long seconds) {
    return create(seconds, 0);
  }

  /** {@code nanoAdjustment} may be outside 0..999,999,999; it is normalised into the seconds. */
  public static Duration ofSeconds(long seconds, long nanoAdjustment) {
    long secs = Math.addExact(seconds, Math.floorDiv(nanoAdjustment, NANOS_PER_SECOND));
    int nos = (int) Math.floorMod(nanoAdjustment, NANOS_PER_SECOND);
    return create(secs, nos);
  }

  public static Duration ofMillis(long millis) {
    long secs = millis / 1000;
    int mos = (int) (millis % 1000);
    if (mos < 0) {
      mos += 1000;
      secs--;
    }
    return create(secs, mos * 1_000_000);
  }

  public static Duration ofNanos(long nanos) {
    long secs = nanos / NANOS_PER_SECOND;
    int nos = (int) (nanos % NANOS_PER_SECOND);
    if (nos < 0) {
      nos = (int) (nos + NANOS_PER_SECOND);
      secs--;
    }
    return create(secs, nos);
  }

  public static Duration of(long amount, TemporalUnit unit) {
    return ZERO.plus(amount, unit);
  }

  /** The duration from {@code startInclusive} to {@code endExclusive}. */
  public static Duration between(Temporal startInclusive, Temporal endExclusive) {
    try {
      return ofNanos(startInclusive.until(endExclusive, ChronoUnit.NANOS));
    } catch (ArithmeticException ex) {
      // Too far apart for a long of nanoseconds: whole seconds, then the nano remainder.
      long secs = startInclusive.until(endExclusive, ChronoUnit.SECONDS);
      long nanos = nanoOf(endExclusive) - nanoOf(startInclusive);
      if (secs > 0 && nanos < 0) {
        secs++;
      } else if (secs < 0 && nanos > 0) {
        secs--;
      }
      return ofSeconds(secs, nanos);
    }
  }

  private static long nanoOf(Temporal t) {
    if (t instanceof Instant) {
      return ((Instant) t).getNano();
    }
    if (t instanceof LocalTime) {
      return ((LocalTime) t).getNano();
    }
    if (t instanceof LocalDateTime) {
      return ((LocalDateTime) t).getNano();
    }
    return 0;
  }

  public long getSeconds() {
    return seconds;
  }

  /** The seconds or the nanos, the two units a Duration is made of. */
  @Override
  public long get(TemporalUnit unit) {
    if (unit == ChronoUnit.SECONDS) {
      return seconds;
    }
    if (unit == ChronoUnit.NANOS) {
      return nanos;
    }
    throw new DateTimeException("Unsupported unit: " + unit);
  }

  @Override
  public List<TemporalUnit> getUnits() {
    List<TemporalUnit> units = new ArrayList<>(2);
    units.add(ChronoUnit.SECONDS);
    units.add(ChronoUnit.NANOS);
    return units;
  }

  @Override
  public Temporal addTo(Temporal temporal) {
    if (seconds != 0) {
      temporal = temporal.plus(seconds, ChronoUnit.SECONDS);
    }
    if (nanos != 0) {
      temporal = temporal.plus(nanos, ChronoUnit.NANOS);
    }
    return temporal;
  }

  @Override
  public Temporal subtractFrom(Temporal temporal) {
    if (seconds != 0) {
      temporal = temporal.minus(seconds, ChronoUnit.SECONDS);
    }
    if (nanos != 0) {
      temporal = temporal.minus(nanos, ChronoUnit.NANOS);
    }
    return temporal;
  }

  public int getNano() {
    return nanos;
  }

  public boolean isZero() {
    return (seconds | nanos) == 0;
  }

  public boolean isNegative() {
    return seconds < 0;
  }

  public Duration plus(Duration duration) {
    return plus(duration.seconds, duration.nanos);
  }

  public Duration plus(long amountToAdd, TemporalUnit unit) {
    if (unit == ChronoUnit.DAYS) {
      return plus(Math.multiplyExact(amountToAdd, SECONDS_PER_DAY), 0);
    }
    if (unit.isDurationEstimated()) {
      throw new DateTimeException("Unit must not have an estimated duration");
    }
    if (amountToAdd == 0) {
      return this;
    }
    Duration unitDur = unit.getDuration();
    long secs = Math.multiplyExact(unitDur.seconds, amountToAdd);
    long nanos = Math.multiplyExact(unitDur.nanos, amountToAdd);
    return plus(secs, 0).plus(0, nanos);
  }

  public Duration plusDays(long daysToAdd) {
    return plus(Math.multiplyExact(daysToAdd, SECONDS_PER_DAY), 0);
  }

  public Duration plusHours(long hoursToAdd) {
    return plus(Math.multiplyExact(hoursToAdd, 3600L), 0);
  }

  public Duration plusMinutes(long minutesToAdd) {
    return plus(Math.multiplyExact(minutesToAdd, 60L), 0);
  }

  public Duration plusSeconds(long secondsToAdd) {
    return plus(secondsToAdd, 0);
  }

  public Duration plusMillis(long millisToAdd) {
    return plus(millisToAdd / 1000, (millisToAdd % 1000) * 1_000_000);
  }

  public Duration plusNanos(long nanosToAdd) {
    return plus(0, nanosToAdd);
  }

  private Duration plus(long secondsToAdd, long nanosToAdd) {
    if ((secondsToAdd | nanosToAdd) == 0) {
      return this;
    }
    long epochSec = Math.addExact(seconds, secondsToAdd);
    epochSec = Math.addExact(epochSec, nanosToAdd / NANOS_PER_SECOND);
    nanosToAdd = nanosToAdd % NANOS_PER_SECOND;
    long nanoAdjustment = nanos + nanosToAdd;
    return ofSeconds(epochSec, nanoAdjustment);
  }

  public Duration minus(Duration duration) {
    long secsToSubtract = duration.seconds;
    int nanosToSubtract = duration.nanos;
    if (secsToSubtract == Long.MIN_VALUE) {
      return plus(Long.MAX_VALUE, -nanosToSubtract).plus(1, 0);
    }
    return plus(-secsToSubtract, -nanosToSubtract);
  }

  public Duration minus(long amountToSubtract, TemporalUnit unit) {
    return amountToSubtract == Long.MIN_VALUE
        ? plus(Long.MAX_VALUE, unit).plus(1, unit)
        : plus(-amountToSubtract, unit);
  }

  public Duration minusDays(long daysToSubtract) {
    return plusDays(-daysToSubtract);
  }

  public Duration minusHours(long hoursToSubtract) {
    return plusHours(-hoursToSubtract);
  }

  public Duration minusMinutes(long minutesToSubtract) {
    return plusMinutes(-minutesToSubtract);
  }

  public Duration minusSeconds(long secondsToSubtract) {
    return plusSeconds(-secondsToSubtract);
  }

  public Duration minusMillis(long millisToSubtract) {
    return plusMillis(-millisToSubtract);
  }

  public Duration minusNanos(long nanosToSubtract) {
    return plusNanos(-nanosToSubtract);
  }

  public Duration multipliedBy(long multiplicand) {
    if (multiplicand == 0) {
      return ZERO;
    }
    if (multiplicand == 1) {
      return this;
    }
    long secs = Math.multiplyExact(seconds, multiplicand);
    long nanosScaled = Math.multiplyExact((long) nanos, multiplicand);
    return ofSeconds(secs, 0).plus(0, nanosScaled);
  }

  public Duration dividedBy(long divisor) {
    if (divisor == 0) {
      throw new ArithmeticException("Cannot divide by zero");
    }
    if (divisor == 1) {
      return this;
    }
    // Total nanoseconds divided exactly, for the |seconds| < 2^33 that fit a long in nanos.
    long totalNanos = Math.addExact(Math.multiplyExact(seconds, NANOS_PER_SECOND), nanos);
    return ofNanos(totalNanos / divisor);
  }

  public Duration negated() {
    return multipliedBy(-1);
  }

  public Duration abs() {
    return isNegative() ? negated() : this;
  }

  public long toDays() {
    return seconds / SECONDS_PER_DAY;
  }

  public long toHours() {
    return seconds / 3600;
  }

  public long toMinutes() {
    return seconds / 60;
  }

  public long toMillis() {
    long millis = Math.multiplyExact(seconds, 1000L);
    return Math.addExact(millis, nanos / 1_000_000);
  }

  public long toNanos() {
    long total = Math.multiplyExact(seconds, NANOS_PER_SECOND);
    return Math.addExact(total, nanos);
  }

  @Override
  public int compareTo(Duration other) {
    if (seconds != other.seconds) {
      return seconds < other.seconds ? -1 : 1;
    }
    return nanos - other.nanos;
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (obj instanceof Duration) {
      Duration other = (Duration) obj;
      return seconds == other.seconds && nanos == other.nanos;
    }
    return false;
  }

  @Override
  public int hashCode() {
    return ((int) (seconds ^ (seconds >>> 32))) + (51 * nanos);
  }

  /** ISO-8601, as the JDK prints it: {@code PT8H6M12.345S}, {@code PT0S}, {@code PT-1M}. */
  @Override
  public String toString() {
    if (seconds == 0 && nanos == 0) {
      return "PT0S";
    }
    long effectiveTotalSecs = seconds;
    if (seconds < 0 && nanos > 0) {
      effectiveTotalSecs++;
    }
    long hours = effectiveTotalSecs / 3600;
    int minutes = (int) ((effectiveTotalSecs % 3600) / 60);
    int secs = (int) (effectiveTotalSecs % 60);
    StringBuilder buf = new StringBuilder(24);
    buf.append("PT");
    if (hours != 0) {
      buf.append(hours).append('H');
    }
    if (minutes != 0) {
      buf.append(minutes).append('M');
    }
    if (secs == 0 && nanos == 0 && buf.length() > 2) {
      return buf.toString();
    }
    if (seconds < 0 && nanos > 0) {
      if (secs == 0) {
        buf.append("-0");
      } else {
        buf.append(secs);
      }
    } else {
      buf.append(secs);
    }
    if (nanos > 0) {
      int pos = buf.length();
      if (seconds < 0) {
        buf.append(2 * NANOS_PER_SECOND - nanos);
      } else {
        buf.append(nanos + NANOS_PER_SECOND);
      }
      // Drop trailing zeros of the fraction, then turn the leading "1" into the point.
      String s = buf.toString();
      int end = s.length();
      while (s.charAt(end - 1) == '0') {
        end--;
      }
      buf = new StringBuilder(s.substring(0, pos));
      buf.append('.').append(s.substring(pos + 1, end));
    }
    buf.append('S');
    return buf.toString();
  }
}
