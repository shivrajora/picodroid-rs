// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.functions;

import kotlin.Function;

/**
 * A Kotlin lambda or function value taking 2 argument(s); the SAM of the {@code invokedynamic}
 * kotlinc emits.
 */
public interface Function2<P1, P2, R> extends Function<R> {
  R invoke(P1 p1, P2 p2);
}
