// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.functions;

import kotlin.Function;

/**
 * A Kotlin lambda or function value taking 1 argument(s); the SAM of the {@code invokedynamic}
 * kotlinc emits.
 */
public interface Function1<P1, R> extends Function<R> {
  R invoke(P1 p1);
}
