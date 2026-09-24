// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import claudeusage.NetTestConfig;
import claudeusage.util.TimeFormat;
import picodroid.app.Service;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Intent;
import picodroid.content.SharedPreferences;
import picodroid.net.NetworkInfo;
import picodroid.os.IBinder;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * The started-and-bound Service that owns the numbers and the two background threads. One polls the
 * bridge. The other nudges the UI once a second so countdowns and the staleness display keep
 * moving; it is separate so that a fetch that blocks, or never returns, cannot freeze the screen on
 * numbers that look live.
 *
 * <p>Started so the numbers stay warm whichever screen is showing; bound so the Activity can read
 * them through the {@link LocalBinder}. The bridge's address is a preference, {@link
 * #KEY_BRIDGE_HOST}, defaulting to the build-time host.
 *
 * <p>Threading: the poll thread never touches the fields the UI reads. It hands each result to the
 * main thread in a posted Runnable, and everything below the "main thread" banner is confined to
 * it. The flags that cross are volatile; the poll thread idles on {@link #lock}.
 */
public final class UsageService extends Service {
  public static final String TAG = "ClaudeUsage";

  /** The preferences file the app's settings live in. */
  public static final String PREFS = "settings";

  /**
   * Host or address of the bridge, with an optional {@code :port}; else {@link UsageFetcher#PORT}.
   */
  public static final String KEY_BRIDGE_HOST = "bridge_host";

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

  public static class LocalBinder implements IBinder {
    public UsageService service;
  }

  private final LocalBinder binder = new LocalBinder();

  // ── Crossing threads ───────────────────────────────────────────────────────

  /** The poll thread waits on this between attempts; a refresh request or destroy wakes it. */
  private final Object lock = new Object();

  private volatile boolean running;
  private volatile boolean refreshRequested;
  private volatile boolean listening;
  private String address;
  private String url;

  // ── Main thread only ───────────────────────────────────────────────────────

  private Listener listener;
  private UsageSnapshot snapshot;
  private LinkState linkState = LinkState.JOINING;
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

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  @Override
  public void onCreate() {
    super.onCreate();
    binder.service = this;
    SharedPreferences prefs = getSharedPreferences(PREFS, MODE_PRIVATE);
    String host = prefs.getString(KEY_BRIDGE_HOST, NetTestConfig.HOST);
    // A host may carry its own port ("192.168.1.5:8790"): a dev PC whose live bridge already
    // owns 8787 runs a demo bridge for the simulator beside it.
    address = host.indexOf(':') >= 0 ? host : host + ":" + UsageFetcher.PORT;
    url = "http://" + address + "/u";
    running = true;
    new Thread(this::pollLoop, "usage-poll").start();
    new Thread(this::tickLoop, "usage-tick").start();
    Log.i(TAG, "service up, bridge " + address);
  }

  @Override
  public int onStartCommand(Intent intent, int flags, int startId) {
    return START_STICKY;
  }

  @Override
  public IBinder onBind(Intent intent) {
    return binder;
  }

  @Override
  public void onDestroy() {
    listener = null;
    listening = false;
    running = false;
    synchronized (lock) {
      lock.notifyAll();
    }
    super.onDestroy();
  }

  // ── Client API (main thread) ───────────────────────────────────────────────

  public void setListener(Listener l) {
    listener = l;
    listening = l != null;
  }

  /** X button: fetch now rather than at the next poll. */
  public void refreshNow() {
    refreshRequested = true;
    synchronized (lock) {
      lock.notifyAll();
    }
  }

  /** Shown on the status screen so a wrong address is obvious. */
  public String bridgeAddress() {
    return address;
  }

  public UsageSnapshot snapshot() {
    return snapshot;
  }

  /** The link state to present: an overdue fetch counts as an unreachable PC. */
  public LinkState linkState() {
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

  private void applyResult(LinkState state, UsageSnapshot fresh, int retryMs) {
    syncing = false;
    nextAttemptElapsedMs = SystemClock.elapsedRealtime() + retryMs;
    if (state != linkState) {
      Log.i(TAG, "state -> " + state.name());
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

  private void applyLinkOnly(LinkState state) {
    if (state != linkState) {
      Log.i(TAG, "state -> " + state.name());
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
    while (running) {
      SystemClock.sleep(TICK_MS);
      if (running && listening) {
        Executors.mainExecutor().execute(tick);
      }
    }
  }

  // ── Poll thread ────────────────────────────────────────────────────────────

  private void pollLoop() {
    int failures = 0;
    boolean everConnected = false;
    while (running) {
      // Consumed on every pass, the offline one included: a request left set makes idle() return
      // at once, and while the link is down that spun this loop, flooding the main queue.
      refreshRequested = false;
      if (!NetworkInfo.isConnected()) {
        final LinkState state = everConnected ? LinkState.NO_WIFI : LinkState.JOINING;
        Executors.mainExecutor().execute(() -> applyLinkOnly(state));
        idle(everConnected ? 5000 : 500);
        continue;
      }
      everConnected = true;
      Executors.mainExecutor().execute(this::applySyncing);

      final UsageSnapshot fresh = new UsageSnapshot();
      final LinkState state = UsageFetcher.fetch(url, fresh);
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

  /** Waits up to {@code ms}, cut short by a refresh request or the Service going away. */
  private void idle(int ms) {
    long until = SystemClock.elapsedRealtime() + ms;
    synchronized (lock) {
      while (running && !refreshRequested) {
        long left = until - SystemClock.elapsedRealtime();
        if (left <= 0) {
          return;
        }
        try {
          lock.wait(left);
        } catch (InterruptedException e) {
          return;
        }
      }
    }
  }
}
