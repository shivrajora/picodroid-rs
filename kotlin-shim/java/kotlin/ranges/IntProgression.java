// SPDX-License-Identifier: GPL-3.0-only
package kotlin.ranges;

import kotlin.collections.IntIterator;

/**
 * {@code a..b step s} / {@code a downTo b} as a value. {@code for} loops over a progression
 * <em>value</em> read {@code first}/{@code last}/{@code step} and iterate inline; only the {@code
 * Iterable} forms ({@code toList()}, {@code map {}}, {@code sum()}) go through {@link #iterator()},
 * whose declared {@link IntIterator} return type makes javac emit the {@code Iterator} bridge
 * kotlinc calls.
 */
public class IntProgression implements Iterable<Integer> {
  private final int first;
  private final int last;
  private final int step;

  public IntProgression(int start, int endInclusive, int step) {
    if (step == 0) {
      throw new IllegalArgumentException("Step must be non-zero.");
    }
    this.first = start;
    this.last =
        kotlin.internal.ProgressionUtilKt.getProgressionLastElement(start, endInclusive, step);
    this.step = step;
  }

  public final int getFirst() {
    return first;
  }

  public final int getLast() {
    return last;
  }

  public final int getStep() {
    return step;
  }

  @Override
  public IntIterator iterator() {
    return new IntProgressionIterator(first, last, step);
  }

  public boolean isEmpty() {
    return step > 0 ? first > last : first < last;
  }

  @Override
  public boolean equals(Object other) {
    if (!(other instanceof IntProgression)) {
      return false;
    }
    IntProgression o = (IntProgression) other;
    return (isEmpty() && o.isEmpty()) || (first == o.first && last == o.last && step == o.step);
  }

  @Override
  public int hashCode() {
    return isEmpty() ? -1 : 31 * (31 * first + last) + step;
  }

  @Override
  public String toString() {
    return step > 0
        ? first + ".." + last + " step " + step
        : first + " downTo " + last + " step " + -step;
  }
}
