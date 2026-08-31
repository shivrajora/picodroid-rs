// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * An object reference updated atomically, mirroring {@code
 * java.util.concurrent.atomic.AtomicReference}. {@link #compareAndSet} compares by identity, as the
 * original does.
 */
public class AtomicReference<V> {
  private V value;

  public AtomicReference() {}

  public AtomicReference(V initialValue) {
    value = initialValue;
  }

  public final synchronized V get() {
    return value;
  }

  public final synchronized void set(V newValue) {
    value = newValue;
  }

  public final void lazySet(V newValue) {
    set(newValue);
  }

  public final synchronized V getAndSet(V newValue) {
    V old = value;
    value = newValue;
    return old;
  }

  public final synchronized boolean compareAndSet(V expect, V update) {
    if (value != expect) {
      return false;
    }
    value = update;
    return true;
  }

  @Override
  public String toString() {
    return String.valueOf(get());
  }
}
