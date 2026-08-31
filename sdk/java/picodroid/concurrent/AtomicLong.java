// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/** A {@code long} updated atomically, mirroring {@code java.util.concurrent.atomic.AtomicLong}. */
public class AtomicLong {
  private long value;

  public AtomicLong() {}

  public AtomicLong(long initialValue) {
    value = initialValue;
  }

  public final synchronized long get() {
    return value;
  }

  public final synchronized void set(long newValue) {
    value = newValue;
  }

  public final void lazySet(long newValue) {
    set(newValue);
  }

  public final synchronized long getAndSet(long newValue) {
    long old = value;
    value = newValue;
    return old;
  }

  public final synchronized boolean compareAndSet(long expect, long update) {
    if (value != expect) {
      return false;
    }
    value = update;
    return true;
  }

  public final synchronized long getAndIncrement() {
    long old = value;
    value = value + 1;
    return old;
  }

  public final synchronized long getAndDecrement() {
    long old = value;
    value = value - 1;
    return old;
  }

  public final synchronized long getAndAdd(long delta) {
    long old = value;
    value = value + delta;
    return old;
  }

  public final synchronized long incrementAndGet() {
    value = value + 1;
    return value;
  }

  public final synchronized long decrementAndGet() {
    value = value - 1;
    return value;
  }

  public final synchronized long addAndGet(long delta) {
    value = value + delta;
    return value;
  }

  public long longValue() {
    return get();
  }

  public int intValue() {
    return (int) get();
  }

  @Override
  public String toString() {
    return Long.toString(get());
  }
}
