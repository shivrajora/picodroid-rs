// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.RejectedExecutionException;
import picodroid.os.SystemClock;

/**
 * The {@link ScheduledExecutorService} behind {@link Executors#newSingleThreadScheduledExecutor()}:
 * one worker {@link Thread} of its own running delayed and periodic tasks in due order, the part of
 * {@code java.util.concurrent.ScheduledThreadPoolExecutor} a single thread needs. Pure Java over
 * {@code synchronized} and {@code wait}/{@code notify}.
 *
 * <p>The worker costs a task stack (16 KiB on the RP family) for as long as the executor lives, and
 * its tasks may block but must not touch views. For a timer on the main thread, which costs no
 * stack, use {@link Executors#mainScheduledExecutor()}.
 */
final class ScheduledThreadPoolExecutor implements ScheduledExecutorService {
  private static int poolSeq = 0;

  /** Tasks waiting for their due time, in no particular order; also the monitor. */
  private final ArrayList<Task<?>> queue = new ArrayList<Task<?>>();

  private final Thread worker;
  private boolean shutdown;
  private boolean terminated;

  private static synchronized int nextPoolId() {
    poolSeq = poolSeq + 1;
    return poolSeq;
  }

  ScheduledThreadPoolExecutor() {
    worker = new Thread(this::work, "scheduled-" + nextPoolId() + "-thread-1");
    worker.start();
  }

  /** One scheduled task: a {@link FutureTask} with a due time and, when periodic, a period. */
  private static final class Task<V> extends FutureTask<V> implements ScheduledFuture<V> {
    private final ScheduledThreadPoolExecutor owner;

    /** 0 for one shot; positive for a fixed rate; negative for a fixed delay, as the JDK has it. */
    private final long periodMs;

    /** {@code SystemClock.elapsedRealtime()} of the next run; guarded by the owner's queue. */
    long dueMs;

    Task(ScheduledThreadPoolExecutor owner, Callable<V> callable, long dueMs) {
      super(callable);
      this.owner = owner;
      this.periodMs = 0;
      this.dueMs = dueMs;
    }

    Task(ScheduledThreadPoolExecutor owner, Runnable runnable, long dueMs, long periodMs) {
      super(runnable, null);
      this.owner = owner;
      this.periodMs = periodMs;
      this.dueMs = dueMs;
    }

    boolean isPeriodic() {
      return periodMs != 0;
    }

    @Override
    public void run() {
      if (periodMs == 0) {
        super.run();
      } else if (runAndReset()) {
        owner.again(this, periodMs);
      }
    }

    /** On completion or cancel: leave the queue. */
    @Override
    protected void done() {
      owner.forget(this);
    }

    @Override
    public long getDelay(TimeUnit unit) {
      if (isDone()) {
        return 0;
      }
      return unit.convert(owner.dueOf(this) - SystemClock.elapsedRealtime(), TimeUnit.MILLISECONDS);
    }
  }

  private void work() {
    while (true) {
      Task<?> task = null;
      synchronized (queue) {
        while (task == null) {
          Task<?> next = earliest();
          if (next == null) {
            if (shutdown) {
              terminated = true;
              queue.notifyAll();
              return;
            }
            await(0);
            continue;
          }
          long left = next.dueMs - SystemClock.elapsedRealtime();
          if (left <= 0) {
            queue.remove(next);
            task = next;
          } else {
            await(left);
          }
        }
      }
      task.run(); // a FutureTask keeps what its computation throws
    }
  }

  /** Waits on the queue, forever when {@code ms} is 0; holds the queue's monitor. */
  private void await(long ms) {
    try {
      if (ms > 0) {
        queue.wait(ms);
      } else {
        queue.wait();
      }
    } catch (InterruptedException e) {
      // shutdownNow interrupts the worker; the loop re-reads the flags.
    }
  }

  /** The queued task due first, or null; holds the queue's monitor. */
  private Task<?> earliest() {
    Task<?> first = null;
    for (int i = 0; i < queue.size(); i++) {
      Task<?> t = queue.get(i);
      if (first == null || t.dueMs < first.dueMs) {
        first = t;
      }
    }
    return first;
  }

  /** A periodic task finished a run: queue the next one, unless the executor is shutting down. */
  void again(Task<?> task, long periodMs) {
    long now = SystemClock.elapsedRealtime();
    boolean stopped;
    synchronized (queue) {
      stopped = shutdown;
    }
    if (stopped) {
      task.cancel(false); // a periodic task ends with its executor, as in the JDK
      return;
    }
    synchronized (queue) {
      if (periodMs > 0) {
        // Fixed rate keeps to the schedule; a run so late that the next is already due is
        // followed by one a full period later, not by a burst.
        long next = task.dueMs + periodMs;
        task.dueMs = next > now ? next : now + periodMs;
      } else {
        task.dueMs = now - periodMs;
      }
      queue.add(task);
      queue.notifyAll();
    }
    if (task.isDone()) {
      forget(task); // cancelled between the run and the re-queue
    }
  }

  void forget(Task<?> task) {
    synchronized (queue) {
      queue.remove(task);
      queue.notifyAll();
    }
  }

  long dueOf(Task<?> task) {
    synchronized (queue) {
      return task.dueMs;
    }
  }

  private static long dueIn(long delay, TimeUnit unit) {
    long ms = unit.toMillis(delay);
    return SystemClock.elapsedRealtime() + (ms < 0 ? 0 : ms);
  }

  private <V> Task<V> enqueue(Task<V> task) {
    synchronized (queue) {
      if (shutdown) {
        throw new RejectedExecutionException("executor has been shut down");
      }
      queue.add(task);
      queue.notifyAll();
    }
    return task;
  }

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
    return enqueue(new Task<Object>(this, command, dueIn(delay, unit), 0L));
  }

  @Override
  public <V> ScheduledFuture<V> schedule(Callable<V> callable, long delay, TimeUnit unit) {
    return enqueue(new Task<V>(this, callable, dueIn(delay, unit)));
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
    return enqueue(
        new Task<Object>(this, command, dueIn(initialDelay, unit), unit.toMillis(period)));
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
    return enqueue(
        new Task<Object>(this, command, dueIn(initialDelay, unit), -unit.toMillis(delay)));
  }

  /** The JDK's default policy: one-shot tasks still due run, periodic ones stop. */
  @Override
  public void shutdown() {
    cancelQueued(true);
  }

  @Override
  public List<Runnable> shutdownNow() {
    List<Runnable> cancelled = cancelQueued(false);
    worker.interrupt();
    return cancelled;
  }

  /** Cancels the queued tasks, all of them or only the periodic ones; returns those cancelled. */
  private List<Runnable> cancelQueued(boolean periodicOnly) {
    ArrayList<Task<?>> victims;
    synchronized (queue) {
      shutdown = true;
      victims = new ArrayList<Task<?>>(queue);
      queue.notifyAll();
    }
    ArrayList<Runnable> cancelled = new ArrayList<Runnable>();
    for (int i = 0; i < victims.size(); i++) {
      Task<?> t = victims.get(i);
      if ((!periodicOnly || t.isPeriodic()) && t.cancel(false)) {
        cancelled.add(t);
      }
    }
    return cancelled;
  }

  @Override
  public boolean isShutdown() {
    synchronized (queue) {
      return shutdown;
    }
  }

  @Override
  public boolean isTerminated() {
    synchronized (queue) {
      return terminated;
    }
  }

  @Override
  public boolean awaitTermination(long timeout, TimeUnit unit) throws InterruptedException {
    long deadline = SystemClock.elapsedRealtime() + unit.toMillis(timeout);
    synchronized (queue) {
      while (!terminated) {
        long left = deadline - SystemClock.elapsedRealtime();
        if (left <= 0) {
          return false;
        }
        queue.wait(left);
      }
      return true;
    }
  }
}
