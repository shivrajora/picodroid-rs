// SPDX-License-Identifier: GPL-3.0-only
package kotlin.ranges;

/** The protocol {@code coerceIn(range)} reads; {@link IntRange} is the only implementor shipped. */
public interface ClosedRange<T extends Comparable<? super T>> {
  T getStart();

  T getEndInclusive();

  boolean isEmpty();
}
