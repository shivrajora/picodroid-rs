// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

import kotlin.jvm.functions.Function0;

/**
 * The {@link Lazy} behind {@code lazy { }}: the initializer runs once, under the instance's
 * monitor, and is dropped afterwards. A boolean flag replaces the real implementation's {@code
 * UNINITIALIZED_VALUE} sentinel object (one class fewer to parse).
 */
final class SynchronizedLazyImpl<T> implements Lazy<T> {
  private Function0<? extends T> initializer;
  private Object value;
  private boolean initialized;

  SynchronizedLazyImpl(Function0<? extends T> initializer) {
    this.initializer = initializer;
  }

  @Override
  @SuppressWarnings("unchecked")
  public T getValue() {
    synchronized (this) {
      if (!initialized) {
        value = initializer.invoke();
        initialized = true;
        initializer = null;
      }
      return (T) value;
    }
  }

  @Override
  public boolean isInitialized() {
    synchronized (this) {
      return initialized;
    }
  }

  @Override
  public String toString() {
    return isInitialized() ? String.valueOf(value) : "Lazy value not initialized yet.";
  }
}
