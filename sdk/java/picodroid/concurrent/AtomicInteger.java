// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * An {@code int} updated atomically, mirroring {@code java.util.concurrent.atomic.AtomicInteger}.
 * Atomicity comes from {@code synchronized} — every Java task runs at one priority on one core, so
 * a monitor is the cheapest correct primitive here.
 */
public class AtomicInteger {
  private int value;

  public AtomicInteger() {}

  public AtomicInteger(int initialValue) {
    value = initialValue;
  }

  public final synchronized int get() {
    return value;
  }

  public final synchronized void set(int newValue) {
    value = newValue;
  }

  public final void lazySet(int newValue) {
    set(newValue);
  }

  public final synchronized int getAndSet(int newValue) {
    int old = value;
    value = newValue;
    return old;
  }

  public final synchronized boolean compareAndSet(int expect, int update) {
    if (value != expect) {
      return false;
    }
    value = update;
    return true;
  }

  public final synchronized int getAndIncrement() {
    return value++;
  }

  public final synchronized int getAndDecrement() {
    return value--;
  }

  public final synchronized int getAndAdd(int delta) {
    int old = value;
    value = value + delta;
    return old;
  }

  public final synchronized int incrementAndGet() {
    value = value + 1;
    return value;
  }

  public final synchronized int decrementAndGet() {
    value = value - 1;
    return value;
  }

  public final synchronized int addAndGet(int delta) {
    value = value + delta;
    return value;
  }

  public int intValue() {
    return get();
  }

  public long longValue() {
    return get();
  }

  @Override
  public String toString() {
    return Integer.toString(get());
  }
}
