// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.List;

/**
 * An {@link Executor} with a lifecycle and {@link Future}-returning submission, mirroring {@code
 * java.util.concurrent.ExecutorService}. Obtain one from {@link Executors#newFixedThreadPool(int)}
 * or {@link Executors#newSingleThreadExecutor()}.
 */
public interface ExecutorService extends Executor {
  Future<?> submit(Runnable task);

  <T> Future<T> submit(Callable<T> task);

  /** Stops accepting tasks; queued tasks still run. */
  void shutdown();

  /** Stops accepting tasks, interrupts the workers, and returns the tasks that never started. */
  List<Runnable> shutdownNow();

  boolean isShutdown();

  /** True once shut down and every worker has exited. */
  boolean isTerminated();

  /** Waits for termination after a {@link #shutdown()}; false if the timeout elapsed first. */
  boolean awaitTermination(long timeout, TimeUnit unit) throws InterruptedException;
}
