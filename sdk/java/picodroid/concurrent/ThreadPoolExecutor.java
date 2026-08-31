// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.RejectedExecutionException;
import picodroid.util.Log;

/**
 * A fixed pool of worker {@link Thread}s draining an unbounded FIFO queue — the part of {@code
 * java.util.concurrent.ThreadPoolExecutor} that {@link Executors#newFixedThreadPool(int)} needs.
 * Pure Java over {@code synchronized} and {@code wait}/{@code notify}; nothing here is native.
 */
final class ThreadPoolExecutor implements ExecutorService {
  private static int poolSeq = 0;

  private final ArrayList<Runnable> queue = new ArrayList<Runnable>();
  private final Thread[] workers;
  private boolean shutdown;

  /** Workers that have not yet exited. */
  private int alive;

  private static synchronized int nextPoolId() {
    poolSeq = poolSeq + 1;
    return poolSeq;
  }

  ThreadPoolExecutor(int nThreads) {
    int pool = nextPoolId();
    workers = new Thread[nThreads];
    alive = nThreads;
    for (int i = 0; i < nThreads; i++) {
      workers[i] = new Thread(this::worker, "pool-" + pool + "-thread-" + (i + 1));
      workers[i].start();
    }
  }

  private void worker() {
    while (true) {
      Runnable task;
      synchronized (queue) {
        while (queue.isEmpty() && !shutdown) {
          try {
            queue.wait();
          } catch (InterruptedException e) {
            // shutdownNow interrupts the workers; re-check the flags.
          }
        }
        if (queue.isEmpty()) {
          alive = alive - 1;
          queue.notifyAll();
          return;
        }
        task = queue.remove(0);
      }
      try {
        task.run();
      } catch (Throwable t) {
        Log.e("ThreadPoolExecutor", "task threw: " + t);
      }
    }
  }

  @Override
  public void execute(Runnable command) {
    if (command == null) {
      throw new NullPointerException();
    }
    synchronized (queue) {
      if (shutdown) {
        throw new RejectedExecutionException("executor has been shut down");
      }
      queue.add(command);
      queue.notify();
    }
  }

  @Override
  public Future<?> submit(Runnable task) {
    FutureTask<Object> f = new FutureTask<Object>(task, null);
    execute(f);
    return f;
  }

  @Override
  public <T> Future<T> submit(Callable<T> task) {
    FutureTask<T> f = new FutureTask<T>(task);
    execute(f);
    return f;
  }

  @Override
  public void shutdown() {
    synchronized (queue) {
      shutdown = true;
      queue.notifyAll();
    }
  }

  @Override
  public List<Runnable> shutdownNow() {
    ArrayList<Runnable> drained = new ArrayList<Runnable>();
    synchronized (queue) {
      shutdown = true;
      for (int i = 0; i < queue.size(); i++) {
        drained.add(queue.get(i));
      }
      queue.clear();
      queue.notifyAll();
    }
    for (int i = 0; i < workers.length; i++) {
      workers[i].interrupt();
    }
    return drained;
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
      return shutdown && alive == 0;
    }
  }

  @Override
  public boolean awaitTermination(long timeout, TimeUnit unit) throws InterruptedException {
    long deadline = System.currentTimeMillis() + unit.toMillis(timeout);
    synchronized (queue) {
      while (!(shutdown && alive == 0)) {
        long left = deadline - System.currentTimeMillis();
        if (left <= 0) {
          return false;
        }
        queue.wait(left);
      }
      return true;
    }
  }
}
