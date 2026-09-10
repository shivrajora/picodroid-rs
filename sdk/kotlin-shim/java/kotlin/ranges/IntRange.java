// SPDX-License-Identifier: GPL-3.0-only
package kotlin.ranges;

/** {@code a..b} as a value (also what {@code until} and {@code indices} return). */
public final class IntRange extends IntProgression implements ClosedRange<Integer> {
  public IntRange(int start, int endInclusive) {
    super(start, endInclusive, 1);
  }

  @Override
  public Integer getStart() {
    return Integer.valueOf(getFirst());
  }

  @Override
  public Integer getEndInclusive() {
    return Integer.valueOf(getLast());
  }

  public boolean contains(int value) {
    return getFirst() <= value && value <= getLast();
  }

  @Override
  public boolean isEmpty() {
    return getFirst() > getLast();
  }

  @Override
  public boolean equals(Object other) {
    if (!(other instanceof IntRange)) {
      return false;
    }
    IntRange o = (IntRange) other;
    return (isEmpty() && o.isEmpty()) || (getFirst() == o.getFirst() && getLast() == o.getLast());
  }

  @Override
  public int hashCode() {
    return isEmpty() ? -1 : 31 * getFirst() + getLast();
  }

  @Override
  public String toString() {
    return getFirst() + ".." + getLast();
  }
}
