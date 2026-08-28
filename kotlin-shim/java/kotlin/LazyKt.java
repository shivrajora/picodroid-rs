// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

import kotlin.jvm.functions.Function0;

/**
 * Facade for {@code kotlin.Lazy.kt}: plain {@code lazy { }} only (no {@code LazyThreadSafetyMode}).
 */
public final class LazyKt {
  private LazyKt() {}

  public static <T> Lazy<T> lazy(Function0<? extends T> initializer) {
    return new SynchronizedLazyImpl<>(initializer);
  }
}
