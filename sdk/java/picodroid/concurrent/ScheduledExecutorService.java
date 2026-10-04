// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * An {@link ExecutorService} that runs tasks after a delay or periodically, mirroring {@code
 * java.util.concurrent.ScheduledExecutorService}. Obtain one from {@link
 * Executors#mainScheduledExecutor()}, whose tasks run on the main thread, or {@link
 * Executors#newSingleThreadScheduledExecutor()}, which runs them on a thread of its own.
 *
 * <p>Delays are measured on {@code SystemClock.elapsedRealtime()}. The main-thread executor
 * resolves them on the runtime's 16 ms frame tick, so a task runs within a frame of its due time
 * when the main thread is idle. A task that throws is dropped: its future completes exceptionally
 * and, if it was periodic, it is not run again.
 */
public interface ScheduledExecutorService extends ExecutorService {
  /** Runs {@code command} once, {@code delay} from now. */
  ScheduledFuture<?> schedule(Runnable command, long delay, TimeUnit unit);

  /** Calls {@code callable} once, {@code delay} from now; {@code get()} returns its result. */
  <V> ScheduledFuture<V> schedule(Callable<V> callable, long delay, TimeUnit unit);

  /**
   * Runs {@code command} at {@code initialDelay}, then every {@code period} counted from the
   * previous due time. A run that starts late does not shift the schedule; one so late that the
   * next due time has already passed is followed by the next run a full {@code period} later rather
   * than by a burst of catch-up runs.
   */
  ScheduledFuture<?> scheduleAtFixedRate(
      Runnable command, long initialDelay, long period, TimeUnit unit);

  /** Runs {@code command} at {@code initialDelay}, then {@code delay} after each run finishes. */
  ScheduledFuture<?> scheduleWithFixedDelay(
      Runnable command, long initialDelay, long delay, TimeUnit unit);
}
