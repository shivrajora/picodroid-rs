// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.concurrent.CancellationException;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeoutException;

/**
 * A cancellable computation that is both the {@link Runnable} an executor runs and the {@link
 * Future} its submitter holds, mirroring {@code java.util.concurrent.FutureTask}. Completion is
 * signalled with {@code notifyAll} on the task itself.
 */
public class FutureTask<V> implements Runnable, Future<V> {
  private static final int NEW = 0;
  private static final int RUNNING = 1;
  private static final int DONE = 2;
  private static final int CANCELLED = 3;

  private final Callable<V> callable;
  private V result;
  private Throwable error;
  private int state = NEW;

  public FutureTask(Callable<V> callable) {
    if (callable == null) {
      throw new NullPointerException();
    }
    this.callable = callable;
  }

  /** Runs {@code runnable} and reports {@code result} on completion. */
  public FutureTask(Runnable runnable, V result) {
    if (runnable == null) {
      throw new NullPointerException();
    }
    this.callable =
        () -> {
          runnable.run();
          return result;
        };
  }

  @Override
  public void run() {
    synchronized (this) {
      if (state != NEW) {
        return;
      }
      state = RUNNING;
    }
    V v = null;
    Throwable t = null;
    try {
      v = callable.call();
    } catch (Throwable e) {
      t = e;
    }
    boolean completed;
    synchronized (this) {
      // A cancel during the run wins: the result is dropped and cancel already reported done().
      completed = state == RUNNING;
      if (completed) {
        result = v;
        error = t;
        state = DONE;
      }
      notifyAll();
    }
    if (completed) {
      done();
    }
  }

  /**
   * Runs the computation without recording its result, so the task can run again: the way a
   * periodic task runs. Returns true if it ran to the end; false if it threw (the task then
   * completes with that exception) or had been cancelled.
   */
  protected boolean runAndReset() {
    synchronized (this) {
      if (state != NEW) {
        return false;
      }
      state = RUNNING;
    }
    Throwable t = null;
    try {
      callable.call();
    } catch (Throwable e) {
      t = e;
    }
    boolean failed;
    synchronized (this) {
      if (state != RUNNING) {
        return false;
      }
      failed = t != null;
      if (failed) {
        error = t;
        state = DONE;
        notifyAll();
      } else {
        state = NEW;
      }
    }
    if (failed) {
      done();
    }
    return !failed;
  }

  /**
   * Called once, after the task completes or is cancelled, outside the task's monitor. Does nothing
   * here; a subclass overrides it to release what it holds.
   */
  protected void done() {}

  @Override
  public boolean cancel(boolean mayInterruptIfRunning) {
    synchronized (this) {
      if (state == DONE || state == CANCELLED) {
        return false;
      }
      state = CANCELLED;
      notifyAll();
    }
    done();
    return true;
  }

  @Override
  public synchronized boolean isCancelled() {
    return state == CANCELLED;
  }

  @Override
  public synchronized boolean isDone() {
    return state == DONE || state == CANCELLED;
  }

  @Override
  public synchronized V get() throws InterruptedException, ExecutionException {
    while (state == NEW || state == RUNNING) {
      wait();
    }
    return report();
  }

  @Override
  public synchronized V get(long timeout, TimeUnit unit)
      throws InterruptedException, ExecutionException, TimeoutException {
    long deadline = System.currentTimeMillis() + unit.toMillis(timeout);
    while (state == NEW || state == RUNNING) {
      long left = deadline - System.currentTimeMillis();
      if (left <= 0) {
        throw new TimeoutException();
      }
      wait(left);
    }
    return report();
  }

  private V report() throws ExecutionException {
    if (state == CANCELLED) {
      throw new CancellationException();
    }
    if (error != null) {
      throw new ExecutionException(error);
    }
    return result;
  }
}
