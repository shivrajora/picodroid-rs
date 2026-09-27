// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * A delayed, cancellable result from a {@link ScheduledExecutorService}, mirroring {@code
 * java.util.concurrent.ScheduledFuture}. A periodic task's future never completes normally: it
 * completes only when the task throws or is cancelled.
 */
public interface ScheduledFuture<V> extends Future<V> {
  /** Time until the next run, in {@code unit}; zero or negative once due, zero once done. */
  long getDelay(TimeUnit unit);
}
