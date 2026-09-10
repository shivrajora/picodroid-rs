// SPDX-License-Identifier: GPL-3.0-only
package kotlin.ranges;

import java.util.NoSuchElementException;
import kotlin.collections.IntIterator;

final class IntProgressionIterator extends IntIterator {
  private final int step;
  private final int finalElement;
  private boolean hasNext;
  private int next;

  IntProgressionIterator(int first, int last, int step) {
    this.step = step;
    this.finalElement = last;
    this.hasNext = step > 0 ? first <= last : first >= last;
    this.next = hasNext ? first : finalElement;
  }

  @Override
  public boolean hasNext() {
    return hasNext;
  }

  @Override
  public int nextInt() {
    int value = next;
    if (value == finalElement) {
      if (!hasNext) {
        throw new NoSuchElementException();
      }
      hasNext = false;
    } else {
      next += step;
    }
    return value;
  }
}
