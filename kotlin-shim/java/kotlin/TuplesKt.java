// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

/** Facade for {@code kotlin.Tuples.kt}: the {@code to} infix function. */
public final class TuplesKt {
  private TuplesKt() {}

  public static <A, B> Pair<A, B> to(A first, B second) {
    return new Pair<>(first, second);
  }
}
