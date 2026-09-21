// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import claudeusage.ClaudeUsageApp;
import claudeusage.util.TimeFormat;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.net.NetworkInfo;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Owns the two background threads. One polls the bridge. The other nudges the UI once a second so
 * countdowns and the staleness display keep moving; it is separate so that a fetch that blocks, or
 * never returns, cannot freeze the screen on numbers that look live.
 *
 * <p>App-scoped rather than a Service, as picoenvmon's NetworkManager is: there is one process and
 * the numbers should be warm whichever screen is showing.
 *
 * <p>Threading: the poll thread never touches the fields the UI reads. It hands each result to the
 * main thread in a posted Runnable, and everything below the "main thread" banner is confined to
 * it. The two flags that cross are volatile.
 */
public final class UsageRepository implements Runnable {
  private static final String TAG = ClaudeUsageApp.TAG;

  /**
   * The bridge itself only asks Anthropic every three minutes; this just keeps {@code age} fresh.
   */
  private static final int POLL_MS = 60_000;

  /** Quick retries catch a PC that is rebooting or a bridge being restarted... */
  private static final int RETRY_FAST_MS = 15_000;

  /** ...and after this many of them the PC is evidently off, so stop hammering the ARP cache. */
  private static final int FAST_RETRIES = 8;

  private static final int RETRY_SLOW_MS = 60_000;

  /** Two and a half polls without a good reply and the numbers on screen are no longer "live". */
  private static final int STALE_AFTER_MS = 150_000;

  /** Limits the bridge itself has not refreshed for this long count as stale too. */
  private static final int UPSTREAM_STALE_S = 900;

  private static final int SLICE_MS = 250;
  private static final int TICK_MS = 1000;

  /**
   * A fetch is bounded by the connect and read timeouts, 8 s together. One still running after this
   * long is wedged, and the UI reports the PC as unreachable rather than "contacting" forever.
   */
  private static final int SYNC_OVERDUE_MS = 12_000;

  /** One trend sample per this long, {@link #TREND_SLOTS} of them: the last hour. */
  private static final int TREND_PERIOD_MS = 150_000;

  public static final int TREND_SLOTS = 24;

  /** What the UI implements. Both callbacks arrive on the main thread. */
  public interface Listener {
    /** New data, a new link state, or a sync starting or ending. */
    void onUsageChanged();

    /** Once a second while registered. */
    void onTick();
  }

  private static final UsageRepository INSTANCE = new UsageRepository();

  public static UsageRepository get() {
    return INSTANCE;
  }

  private UsageRepository() {}

  // ── Crossing threads ───────────────────────────────────────────────────────

  private volatile boolean refreshRequested;
  private volatile boolean listening;
  private boolean started;

  // ── Main thread only ───────────────────────────────────────────────────────

  private Listener listener;
  private UsageSnapshot snapshot;
  private int linkState = LinkState.JOINING;
  private String linkErr = "";
  private boolean syncing;
  private long syncStartedElapsedMs;
  private long lastGoodElapsedMs = -1;
  private long lastGoodWallMs;
  private long nextAttemptElapsedMs;

  /** Session percent per slot, oldest first; -1 is a gap (no data for that slot). */
  private final int[] trend = new int[TREND_SLOTS];

  private int trendCount;
  private long lastTrendElapsedMs = -1;

  /** Pre-allocated: a lambda here would allocate once a second for as long as the app runs. */
  @SuppressWarnings("UnnecessaryLambda")
  private final Runnable tick =
      () -> {
        recordTrend();
        if (listener != null) {
          listener.onTick();
        }
      };

  public void start() {
    if (started) {
      return;
    }
    started = true;
    new Thread(this).start();
    new Thread(this::tickLoop).start();
  }

  public void setListener(Listener l) {
    listener = l;
    listening = l != null;
  }

  /** X button: fetch now rather than at the next poll. */
  public void refreshNow() {
    refreshRequested = true;
  }

  public UsageSnapshot snapshot() {
    return snapshot;
  }

  /** The link state to present: an overdue fetch counts as an unreachable PC. */
  public int linkState() {
    if (syncing && SystemClock.elapsedRealtime() - syncStartedElapsedMs > SYNC_OVERDUE_MS) {
      return LinkState.PC_OFF;
    }
    return linkState;
  }

  public String linkErr() {
    return linkErr;
  }

  /** True while a fetch is in flight and not yet overdue. */
  public boolean isSyncing() {
    return syncing && SystemClock.elapsedRealtime() - syncStartedElapsedMs <= SYNC_OVERDUE_MS;
  }

  /** Wall-clock time of the last good reply, 0 if there has been none. */
  public long lastGoodWallMs() {
    return lastGoodWallMs;
  }

  /** Milliseconds since the last good reply, -1 if there has been none. */
  public long sinceLastGoodMs() {
    return lastGoodElapsedMs < 0 ? -1 : SystemClock.elapsedRealtime() - lastGoodElapsedMs;
  }

  public int secondsToNextAttempt() {
    long ms = nextAttemptElapsedMs - SystemClock.elapsedRealtime();
    return ms <= 0 ? 0 : (int) ((ms + 999) / 1000);
  }

  /** True when the numbers held are current enough to present as live. */
  public boolean isFresh() {
    if (snapshot == null || !snapshot.hasLimits() || lastGoodElapsedMs < 0) {
      return false;
    }
    if (SystemClock.elapsedRealtime() - lastGoodElapsedMs >= STALE_AFTER_MS) {
      return false;
    }
    return snapshot.ageS >= 0 && snapshot.ageS < UPSTREAM_STALE_S;
  }

  public int trendCount() {
    return trendCount;
  }

  public int trendAt(int i) {
    return trend[i];
  }

  private void recordTrend() {
    long now = SystemClock.elapsedRealtime();
    if (lastTrendElapsedMs >= 0 && now - lastTrendElapsedMs < TREND_PERIOD_MS) {
      return;
    }
    if (snapshot == null && trendCount == 0) {
      return; // nothing yet to put a gap in
    }
    lastTrendElapsedMs = now;
    int value = isFresh() ? snapshot.sessionPct : -1;
    if (trendCount < TREND_SLOTS) {
      trend[trendCount++] = value;
    } else {
      System.arraycopy(trend, 1, trend, 0, TREND_SLOTS - 1);
      trend[TREND_SLOTS - 1] = value;
    }
  }

  private void applyResult(int state, UsageSnapshot fresh, int retryMs) {
    syncing = false;
    nextAttemptElapsedMs = SystemClock.elapsedRealtime() + retryMs;
    if (state != linkState) {
      Log.i(TAG, "state -> " + LinkState.name(state));
    }
    linkState = state;
    linkErr = fresh != null ? fresh.err : "";
    if (fresh != null) {
      if (fresh.bridgeEpochS > 0) {
        TimeFormat.utcOffsetMinutes = fresh.tzMinutes;
        long wall = fresh.bridgeEpochS * 1000L;
        long drift = System.currentTimeMillis() - wall;
        if (drift > 2000 || drift < -2000) {
          SystemClock.setCurrentTimeMillis(wall);
        }
      }
      // A reply without limits (the bridge has never reached Anthropic) must not wipe out good
      // numbers from earlier: they are still the best there is, and the staleness display covers
      // their age.
      if (fresh.hasLimits() || snapshot == null) {
        snapshot = fresh;
      }
      if (state == LinkState.OK) {
        lastGoodElapsedMs = SystemClock.elapsedRealtime();
        lastGoodWallMs = System.currentTimeMillis();
        Log.i(TAG, "sync ok s=" + fresh.sessionPct + " w=" + fresh.weeklyPct);
        if (trendCount == 0) {
          lastTrendElapsedMs = -1; // first sample straight away
          recordTrend();
        }
      }
    }
    if (listener != null) {
      listener.onUsageChanged();
    }
  }

  private void applyLinkOnly(int state) {
    if (state != linkState) {
      Log.i(TAG, "state -> " + LinkState.name(state));
      linkState = state;
      linkErr = "";
      if (listener != null) {
        listener.onUsageChanged();
      }
    }
  }

  private void applySyncing() {
    syncing = true;
    syncStartedElapsedMs = SystemClock.elapsedRealtime();
    if (listener != null) {
      listener.onUsageChanged();
    }
  }

  // ── Ticker thread ──────────────────────────────────────────────────────────

  private void tickLoop() {
    while (true) {
      SystemClock.sleep(TICK_MS);
      if (listening) {
        Executors.mainExecutor().execute(tick);
      }
    }
  }

  // ── Poll thread ────────────────────────────────────────────────────────────

  @Override
  public void run() {
    int failures = 0;
    boolean everConnected = false;
    while (true) {
      if (!NetworkInfo.isConnected()) {
        final int state = everConnected ? LinkState.NO_WIFI : LinkState.JOINING;
        Executors.mainExecutor().execute(() -> applyLinkOnly(state));
        idle(everConnected ? 5000 : 500);
        continue;
      }
      everConnected = true;
      refreshRequested = false;
      Executors.mainExecutor().execute(this::applySyncing);

      final UsageSnapshot fresh = new UsageSnapshot();
      final int state = UsageFetcher.fetch(fresh);
      final boolean gotReply = state == LinkState.OK || state == LinkState.UPSTREAM;
      failures = state == LinkState.OK ? 0 : failures + 1;
      final int wait =
          state == LinkState.OK
              ? POLL_MS
              : (failures <= FAST_RETRIES ? RETRY_FAST_MS : RETRY_SLOW_MS);
      Executors.mainExecutor().execute(() -> applyResult(state, gotReply ? fresh : null, wait));
      idle(wait);
    }
  }

  /** Sleeps up to {@code ms}, cutting short on a refresh request. */
  private void idle(int ms) {
    for (int slept = 0; slept < ms; slept += SLICE_MS) {
      if (refreshRequested) {
        return;
      }
      SystemClock.sleep(SLICE_MS);
    }
  }
}
