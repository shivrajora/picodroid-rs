// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.concurrent.CancellationException;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeoutException;

/** The result of an asynchronous computation, mirroring {@code java.util.concurrent.Future}. */
public interface Future<V> {
  /** Cancels the task if it has not started; a running task is not stopped. */
  boolean cancel(boolean mayInterruptIfRunning);

  boolean isCancelled();

  boolean isDone();

  /**
   * Waits for the result.
   *
   * @throws ExecutionException wrapping whatever the task threw ({@code getCause()})
   * @throws CancellationException if the task was cancelled
   */
  V get() throws InterruptedException, ExecutionException;

  /**
   * Waits at most {@code timeout} for the result.
   *
   * @throws TimeoutException if the result is not ready in time
   */
  V get(long timeout, TimeUnit unit)
      throws InterruptedException, ExecutionException, TimeoutException;
}
