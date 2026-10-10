// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.LinkState;
import claudeusage.data.UsageData;
import claudeusage.data.UsageRepository;
import claudeusage.data.UsageSnapshot;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.Observer;
import picodroid.lifecycle.ViewModel;
import picodroid.os.SystemClock;

/**
 * What the screens show, shared by the Activity and its pages. It owns the data source, the {@link
 * UsageRepository}, and turns what that publishes into one immutable {@link UsageUiState} that the
 * Activity observes for the chrome and each page for itself; nobody sets anything on it. The
 * buttons' requests go back through it.
 *
 * <p>The UI's timing lives here too. Part of what is shown moves with the clock and not with the
 * data: the header's time, the countdowns, numbers going stale. While somebody is observing, a
 * once-a-second task on the main thread looks at the clock and publishes a new state only when one
 * would be painted differently, which on the data screens is once a minute.
 */
public final class UsageViewModel extends ViewModel {
  private static final int TICK_MS = 1000;

  private final UsageRepository repository = UsageRepository.getInstance();
  private final ScheduledExecutorService scheduler = Executors.mainScheduledExecutor();
  private final State state = new State();

  LiveData<UsageUiState> state() {
    return state;
  }

  /** X: fetch now rather than at the next poll. */
  void refreshNow() {
    repository.refreshNow();
  }

  /** X held: look for the bridge on the LAN again, then fetch. */
  void rediscover() {
    repository.rediscover();
  }

  @Override
  protected void onCleared() {
    scheduler.shutdownNow();
  }

  /** The published state: fed by the repository and the clock for as long as it is observed. */
  private final class State extends LiveData<UsageUiState> {
    private final Observer<UsageData> observer = this::derive;

    private ScheduledFuture<?> ticking;

    @Override
    protected void onActive() {
      repository.data().observeForever(observer);
      ticking =
          scheduler.scheduleAtFixedRate(this::onTick, TICK_MS, TICK_MS, TimeUnit.MILLISECONDS);
    }

    @Override
    protected void onInactive() {
      repository.data().removeObserver(observer);
      if (ticking != null) {
        ticking.cancel(false);
        ticking = null;
      }
    }

    private void onTick() {
      derive(repository.data().getValue());
    }

    private void derive(UsageData data) {
      UsageSnapshot snapshot = data.snapshot;
      long elapsed = SystemClock.elapsedRealtime();

      boolean fresh = data.isFresh(elapsed);
      LinkState link = data.stateAt(elapsed);
      boolean syncing = data.syncingAt(elapsed);
      long minute = System.currentTimeMillis() / 60_000L;
      long since = data.sinceLastGoodMs(elapsed);
      long staleMinutes = since < 0 ? -1 : since / 60_000L;
      // The countdown is on the status screen alone; counting it under the data screens would
      // publish a state a second that they paint the same.
      boolean counting = snapshot == null || !snapshot.hasLimits();
      int retrySeconds = counting ? data.secondsToNextAttempt(elapsed) : 0;

      UsageUiState shown = getValue();
      if (shown != null
          && shown.shows(
              snapshot,
              fresh,
              link,
              data.err,
              syncing,
              minute,
              data.lastGoodWallMs,
              staleMinutes,
              data.address,
              retrySeconds,
              data.trend)) {
        return;
      }
      setValue(
          new UsageUiState(
              snapshot,
              fresh,
              link,
              data.err,
              syncing,
              minute,
              data.lastGoodWallMs,
              staleMinutes,
              data.address,
              retrySeconds,
              data.trend));
    }
  }
}
