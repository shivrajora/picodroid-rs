// SPDX-License-Identifier: GPL-3.0-only
package kotlin.collections;

import java.util.Iterator;

/**
 * Primitive iterator over {@code Int}s: kotlinc calls {@link #nextInt()} on a {@code for} over a
 * progression value and {@code next()} (the boxing bridge) through {@code Iterable}.
 */
public abstract class IntIterator implements Iterator<Integer> {
  @Override
  public final Integer next() {
    return Integer.valueOf(nextInt());
  }

  public abstract int nextInt();
}
