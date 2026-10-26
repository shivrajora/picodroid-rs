// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.ViewModel;

/**
 * The wall clock as a face shows it: {@link #now} publishes the time four times a second for as
 * long as a started screen observes it, and not at all otherwise. The clock face and the ring
 * screen each observe their own.
 *
 * <p>Four a second because the face's colon blinks on the half second; the digits only move once a
 * minute, and the face repaints only the segments that changed.
 */
public final class ClockViewModel extends ViewModel {
  private static final long TICK_MS = 250;

  private final ScheduledExecutorService scheduler = Executors.mainScheduledExecutor();
  private final Now now = new Now();

  /** The wall clock, UTC epoch milliseconds, republished every {@link #TICK_MS}. */
  LiveData<Long> now() {
    return now;
  }

  @Override
  protected void onCleared() {
    scheduler.shutdownNow();
  }

  /** Ticks while observed: the task is armed on the first active observer and cancelled after. */
  private final class Now extends LiveData<Long> {
    private ScheduledFuture<?> ticking;

    @Override
    protected void onActive() {
      tick();
      if (ticking == null) {
        ticking =
            scheduler.scheduleAtFixedRate(this::tick, TICK_MS, TICK_MS, TimeUnit.MILLISECONDS);
      }
    }

    @Override
    protected void onInactive() {
      if (ticking != null) {
        ticking.cancel(false);
        ticking = null;
      }
    }

    private void tick() {
      setValue(System.currentTimeMillis());
    }
  }
}
