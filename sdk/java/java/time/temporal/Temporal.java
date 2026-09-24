// SPDX-License-Identifier: GPL-3.0-only
package java.time.temporal;

/**
 * A date, time or both that unit arithmetic applies to: {@code LocalDate}, {@code LocalTime},
 * {@code LocalDateTime} and {@code Instant}. The field-based {@code with(TemporalField, long)} is
 * not served.
 */
public interface Temporal extends TemporalAccessor {
  boolean isSupported(TemporalUnit unit);

  default Temporal plus(TemporalAmount amount) {
    return amount.addTo(this);
  }

  Temporal plus(long amountToAdd, TemporalUnit unit);

  default Temporal minus(TemporalAmount amount) {
    return amount.subtractFrom(this);
  }

  default Temporal minus(long amountToSubtract, TemporalUnit unit) {
    return amountToSubtract == Long.MIN_VALUE
        ? plus(Long.MAX_VALUE, unit).plus(1, unit)
        : plus(-amountToSubtract, unit);
  }

  /** The amount of {@code unit} from this temporal to {@code endExclusive}. */
  long until(Temporal endExclusive, TemporalUnit unit);
}
