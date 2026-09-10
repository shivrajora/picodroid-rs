// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

import kotlin.jvm.internal.Intrinsics;

/** {@code a to b}: a two-element tuple with data-class semantics and {@code (a, b)} formatting. */
public final class Pair<A, B> {
  private final A first;
  private final B second;

  public Pair(A first, B second) {
    this.first = first;
    this.second = second;
  }

  public A getFirst() {
    return first;
  }

  public B getSecond() {
    return second;
  }

  public A component1() {
    return first;
  }

  public B component2() {
    return second;
  }

  @Override
  public String toString() {
    return "(" + first + ", " + second + ")";
  }

  @Override
  public boolean equals(Object other) {
    if (this == other) {
      return true;
    }
    if (!(other instanceof Pair)) {
      return false;
    }
    Pair<?, ?> p = (Pair<?, ?>) other;
    return Intrinsics.areEqual(first, p.first) && Intrinsics.areEqual(second, p.second);
  }

  @Override
  public int hashCode() {
    int h = first == null ? 0 : first.hashCode();
    return h * 31 + (second == null ? 0 : second.hashCode());
  }
}
