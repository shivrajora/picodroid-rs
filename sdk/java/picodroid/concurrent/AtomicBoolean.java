// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * A {@code boolean} updated atomically, mirroring {@code
 * java.util.concurrent.atomic.AtomicBoolean}.
 */
public class AtomicBoolean {
  private boolean value;

  public AtomicBoolean() {}

  public AtomicBoolean(boolean initialValue) {
    value = initialValue;
  }

  public final synchronized boolean get() {
    return value;
  }

  public final synchronized void set(boolean newValue) {
    value = newValue;
  }

  public final void lazySet(boolean newValue) {
    set(newValue);
  }

  public final synchronized boolean getAndSet(boolean newValue) {
    boolean old = value;
    value = newValue;
    return old;
  }

  public final synchronized boolean compareAndSet(boolean expect, boolean update) {
    if (value != expect) {
      return false;
    }
    value = update;
    return true;
  }

  @Override
  public String toString() {
    return get() ? "true" : "false";
  }
}
