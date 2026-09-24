// SPDX-License-Identifier: GPL-3.0-only
package java.time.temporal;

/**
 * Read-only access to a date, time or both. Apps compile against the JDK's interface; this one is
 * the runtime's stand-in so that {@code DateTimeFormatter.format(TemporalAccessor)} keeps its JDK
 * descriptor. Field-by-field access ({@code TemporalField}, {@code ChronoField}) is not served: the
 * formatter reads the concrete {@code LocalDate} / {@code LocalTime} / {@code LocalDateTime}.
 */
public interface TemporalAccessor {
  /** The JDK's fallback body; kept so the interface carries code and is not pure flash. */
  default <R> R query(TemporalQuery<R> query) {
    return query.queryFrom(this);
  }
}
