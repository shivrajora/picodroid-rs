// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.RejectedExecutionException;

/**
 * The {@link ScheduledExecutorService} behind {@link Executors#mainScheduledExecutor()}.
 *
 * <p>Its one thread is the main thread. The runtime keeps a small table of deadlines that its frame
 * tick checks, posting each due task to the main queue, so a scheduled task costs a table slot
 * rather than a 16 KiB task stack and runs where widgets may be touched. The other side of that
 * coin: a task that blocks stalls the UI for as long as it runs, as it would on a {@code Handler},
 * so hand blocking work to {@link Executors#backgroundExecutor()} from the task.
 *
 * <p>{@link #shutdown()} keeps the JDK's default policy — one-shot tasks still due run, periodic
 * ones stop — and {@link #shutdownNow()} cancels everything, which is what an Activity's {@code
 * onDestroy} wants. The table holds sixteen slots shared by every instance.
 */
final class MainScheduledExecutor implements ScheduledExecutorService {
  static final int ONE_SHOT = 0;
  static final int FIXED_RATE = 1;
  static final int FIXED_DELAY = 2;

  /** Tasks armed in the table and not yet done, so shutdown can find them. */
  private final ArrayList<ScheduledFutureTask<?>> armed = new ArrayList<ScheduledFutureTask<?>>();

  private boolean shutdown;

  // The deadline table lives in the runtime (picodroid-core, executors/scheduled.rs).

  /** Arms a slot for {@code task}; returns its id, or -1 when the table is full. */
  static native int schedule0(Runnable task, long delayMs, long periodMs, int kind);

  /** Frees a slot; false when {@code id} no longer names a live one. */
  static native boolean cancel0(int id);

  /** Fixed delay only: the task's run is over, so the next one is {@code period} from now. */
  static native void completed0(int id);

  /** Milliseconds until the slot is due; negative when overdue, zero for a stale id. */
  static native long delay0(int id);

  @Override
  public void execute(Runnable command) {
    schedule(command, 0, TimeUnit.MILLISECONDS);
  }

  @Override
  public Future<?> submit(Runnable task) {
    return schedule(task, 0, TimeUnit.MILLISECONDS);
  }

  @Override
  public <T> Future<T> submit(Callable<T> task) {
    return schedule(task, 0, TimeUnit.MILLISECONDS);
  }

  @Override
  public ScheduledFuture<?> schedule(Runnable command, long delay, TimeUnit unit) {
    if (command == null) {
      throw new NullPointerException();
    }
    return arm(new ScheduledFutureTask<Object>(this, command, null, 0L), unit.toMillis(delay));
  }

  @Override
  public <V> ScheduledFuture<V> schedule(Callable<V> callable, long delay, TimeUnit unit) {
    return arm(new ScheduledFutureTask<V>(this, callable, 0L), unit.toMillis(delay));
  }

  @Override
  public ScheduledFuture<?> scheduleAtFixedRate(
      Runnable command, long initialDelay, long period, TimeUnit unit) {
    if (command == null) {
      throw new NullPointerException();
    }
    if (period <= 0) {
      throw new IllegalArgumentException("period <= 0");
    }
    return arm(
        new ScheduledFutureTask<Object>(this, command, null, unit.toMillis(period)),
        unit.toMillis(initialDelay));
  }

  @Override
  public ScheduledFuture<?> scheduleWithFixedDelay(
      Runnable command, long initialDelay, long delay, TimeUnit unit) {
    if (command == null) {
      throw new NullPointerException();
    }
    if (delay <= 0) {
      throw new IllegalArgumentException("delay <= 0");
    }
    return arm(
        new ScheduledFutureTask<Object>(this, command, null, -unit.toMillis(delay)),
        unit.toMillis(initialDelay));
  }

  private <V> ScheduledFutureTask<V> arm(ScheduledFutureTask<V> task, long delayMs) {
    synchronized (this) {
      if (shutdown) {
        throw new RejectedExecutionException("executor has been shut down");
      }
      armed.add(task);
    }
    task.arm(delayMs < 0 ? 0 : delayMs);
    return task;
  }

  /** Called by a task on completion or cancel, whichever thread that happens on. */
  synchronized void finished(ScheduledFutureTask<?> task) {
    armed.remove(task);
    notifyAll();
  }

  @Override
  public void shutdown() {
    cancelArmed(true);
  }

  @Override
  public List<Runnable> shutdownNow() {
    return cancelArmed(false);
  }

  /** Cancels the armed tasks, all of them or only the periodic ones; returns those cancelled. */
  private List<Runnable> cancelArmed(boolean periodicOnly) {
    ArrayList<ScheduledFutureTask<?>> victims;
    synchronized (this) {
      shutdown = true;
      victims = new ArrayList<ScheduledFutureTask<?>>(armed);
      notifyAll();
    }
    ArrayList<Runnable> cancelled = new ArrayList<Runnable>();
    for (int i = 0; i < victims.size(); i++) {
      ScheduledFutureTask<?> t = victims.get(i);
      if ((!periodicOnly || t.isPeriodic()) && t.cancel(false)) {
        cancelled.add(t);
      }
    }
    return cancelled;
  }

  @Override
  public synchronized boolean isShutdown() {
    return shutdown;
  }

  @Override
  public synchronized boolean isTerminated() {
    return shutdown && armed.isEmpty();
  }

  @Override
  public boolean awaitTermination(long timeout, TimeUnit unit) throws InterruptedException {
    long deadline = System.currentTimeMillis() + unit.toMillis(timeout);
    synchronized (this) {
      while (!(shutdown && armed.isEmpty())) {
        long left = deadline - System.currentTimeMillis();
        if (left <= 0) {
          return false;
        }
        wait(left);
      }
      return true;
    }
  }
}
