// SPDX-License-Identifier: GPL-3.0-only
package java.util;

/**
 * Mirrors {@code java.util.Comparator}: a comparison function imposing a total ordering. A
 * single-method interface, so lambdas work — {@code (a, b) -> a.field - b.field}.
 *
 * <p>Picodroid divergence: the JDK's default/static helpers ({@code reversed()}, {@code
 * comparing(...)}, {@code thenComparing(...)}) are not provided.
 */
public interface Comparator<T> {
  int compare(T o1, T o2);
}
