// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import java.util.concurrent.RejectedExecutionException;

/**
 * A {@link FutureTask} armed in the runtime's deadline table. The table posts it to the main queue
 * when due; {@link #run()} is then the ordinary main-queue dispatch. Mirrors the private {@code
 * ScheduledThreadPoolExecutor.ScheduledFutureTask}.
 */
final class ScheduledFutureTask<V> extends FutureTask<V> implements ScheduledFuture<V> {
  private final MainScheduledExecutor owner;

  /** 0 for one shot; positive for a fixed rate; negative for a fixed delay, as the JDK encodes. */
  private final long periodMs;

  /** The table slot, -1 while unarmed or once done. */
  private int id = -1;

  ScheduledFutureTask(MainScheduledExecutor owner, Callable<V> callable, long periodMs) {
    super(callable);
    this.owner = owner;
    this.periodMs = periodMs;
  }

  ScheduledFutureTask(MainScheduledExecutor owner, Runnable runnable, V result, long periodMs) {
    super(runnable, result);
    this.owner = owner;
    this.periodMs = periodMs;
  }

  boolean isPeriodic() {
    return periodMs != 0;
  }

  /** Takes a table slot. Nobody else holds this task yet, so no cancel can race the arm. */
  void arm(long delayMs) {
    int kind =
        periodMs == 0
            ? MainScheduledExecutor.ONE_SHOT
            : (periodMs > 0 ? MainScheduledExecutor.FIXED_RATE : MainScheduledExecutor.FIXED_DELAY);
    long period = periodMs < 0 ? -periodMs : periodMs;
    int slot = MainScheduledExecutor.schedule0(this, delayMs, period, kind);
    if (slot < 0) {
      owner.finished(this);
      throw new RejectedExecutionException("no free timer slot");
    }
    synchronized (this) {
      id = slot;
    }
  }

  @Override
  public void run() {
    if (periodMs == 0) {
      super.run();
    } else if (runAndReset() && periodMs < 0) {
      // Fixed delay: the next run is counted from now, so tell the table this one is over.
      int slot;
      synchronized (this) {
        slot = id;
      }
      if (slot >= 0) {
        MainScheduledExecutor.completed0(slot);
      }
    }
  }

  /** On completion or cancel: give the slot back and leave the owner's live set. */
  @Override
  protected void done() {
    int slot;
    synchronized (this) {
      slot = id;
      id = -1;
    }
    if (slot >= 0) {
      MainScheduledExecutor.cancel0(slot);
    }
    owner.finished(this);
  }

  @Override
  public long getDelay(TimeUnit unit) {
    int slot;
    synchronized (this) {
      slot = id;
    }
    if (slot < 0) {
      return 0;
    }
    return unit.convert(MainScheduledExecutor.delay0(slot), TimeUnit.MILLISECONDS);
  }
}
