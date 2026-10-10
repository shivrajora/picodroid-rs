// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import claudeusage.BuildConfig;
import claudeusage.util.TimeFormat;
import picodroid.app.Service;
import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.Thread;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.SharedPreferences;
import picodroid.net.ConnectivityManager;
import picodroid.net.Network;
import picodroid.os.IBinder;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * The started Service that keeps the numbers warm: it owns the poll thread that fetches from the
 * bridge and publishes what comes back in the {@link UsageRepository}, which is all the screens
 * know of it. Nothing binds to it. The bridge's address is, in order: the {@link #KEY_BRIDGE_HOST}
 * preference when set; else whatever {@link BridgeDiscovery} finds on the LAN, asked at boot and
 * again while the bridge is unreachable; else the last address it found ({@link
 * #KEY_BRIDGE_FOUND}); else the build-time host.
 *
 * <p>Threading: the poll thread never touches the state the repository publishes. It hands each
 * result to the main thread in a posted Runnable, and everything below the "main thread" banner is
 * confined to it. The flags that cross are volatile; the poll thread idles on {@link #lock}. The
 * link's state comes from {@link ConnectivityManager} on the main thread, as on Android: {@code
 * onAvailable} wakes the poll thread, {@code onLost} publishes the offline state at once and
 * nothing polls the link meanwhile.
 */
public final class UsageService extends Service implements UsageRepository.Poller {
  public static final String TAG = "ClaudeUsage";

  /** The preferences file the app's settings live in. */
  public static final String PREFS = "settings";

  /**
   * Host or address of the bridge, with an optional {@code :port}; else {@link UsageFetcher#PORT}.
   * Set by hand (via {@code pdb}); while present, no discovery runs.
   */
  public static final String KEY_BRIDGE_HOST = "bridge_host";

  /** Where the last broadcast found the bridge, {@code host:port}: a cache, never set by hand. */
  public static final String KEY_BRIDGE_FOUND = "bridge_found";

  /** While the bridge stays unreachable, ask the LAN where it is again this often. */
  private static final int REDISCOVER_MS = 60_000;

  /**
   * The bridge itself only asks Anthropic every three minutes; this just keeps {@code age} fresh.
   */
  private static final int POLL_MS = 60_000;

  /** Quick retries catch a PC that is rebooting or a bridge being restarted... */
  private static final int RETRY_FAST_MS = 15_000;

  /** ...and after this many of them the PC is evidently off, so stop hammering the ARP cache. */
  private static final int FAST_RETRIES = 8;

  private static final int RETRY_SLOW_MS = 60_000;

  /**
   * With the link down there is nothing to poll: {@code onAvailable} wakes the poll thread. This is
   * only how often it looks for itself, in case a wake was missed.
   */
  private static final int LINK_WAIT_MS = 30_000;

  /** One trend sample per this long, {@link #TREND_SLOTS} of them: the last hour. */
  static final int TREND_PERIOD_MS = 150_000;

  public static final int TREND_SLOTS = 24;

  private final UsageRepository repository = UsageRepository.getInstance();

  private ConnectivityManager connectivity;

  /** Main thread, from the framework: the link came up with an address, or dropped. */
  private final ConnectivityManager.NetworkCallback linkWatch =
      new ConnectivityManager.NetworkCallback() {
        @Override
        public void onAvailable(Network network) {
          linkUp = true;
          everConnected = true;
          synchronized (lock) {
            lock.notifyAll();
          }
        }

        @Override
        public void onLost(Network network) {
          linkUp = false;
          applyLinkOnly(LinkState.NO_WIFI);
          synchronized (lock) {
            lock.notifyAll();
          }
        }
      };

  // ── Crossing threads ───────────────────────────────────────────────────────

  /** The poll thread waits on this between attempts; a refresh request or destroy wakes it. */
  private final Object lock = new Object();

  private volatile boolean running;
  private volatile boolean refreshRequested;

  /** X held: probe the LAN for the bridge before the next fetch, answered or not. */
  private volatile boolean rediscoverRequested;

  /** The link, as {@link #linkWatch} last heard it; a change also wakes {@link #idle}. */
  private volatile boolean linkUp;

  /** Once true, a link that is down is one that dropped, not one still joining. */
  private volatile boolean everConnected;

  /**
   * The address being tried, {@code host:port}: pinned, discovered, or the fallback while discovery
   * finds nothing. Null only before the first probe of a unit that has never found the bridge.
   * Written by the poll thread (and by {@code onCreate} before it starts).
   */
  private volatile String address;

  private volatile String url;

  /** Set in {@code onCreate}, before the poll thread starts; both are safe from any thread. */
  private Executor main;

  private SharedPreferences prefs;

  // ── Poll thread only ───────────────────────────────────────────────────────

  /** No pinned address: broadcast for the bridge and follow it. */
  private boolean discover;

  /** The build-time host, tried when nothing answers the broadcast. */
  private String fallback;

  private long lastProbeElapsedMs = -1;

  // ── Main thread only ───────────────────────────────────────────────────────

  /** Samples the trend; a task on the main thread, so it costs no stack of its own. */
  private final ScheduledExecutorService scheduler = Executors.mainScheduledExecutor();

  private UsageSnapshot snapshot;
  private LinkState linkState = LinkState.JOINING;
  private String linkErr = "";
  private long syncStartedElapsedMs = -1;
  private long lastGoodElapsedMs = -1;
  private long lastGoodWallMs;
  private long nextAttemptElapsedMs;

  /** Session percent per sample, oldest first; -1 is a gap. Replaced, never changed in place. */
  private int[] trend = new int[0];

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  @Override
  public void onCreate() {
    super.onCreate();
    main = getMainExecutor();
    prefs = getSharedPreferences(PREFS, MODE_PRIVATE);
    String pinned = prefs.getString(KEY_BRIDGE_HOST, null);
    if (pinned != null) {
      setAddress(withPort(pinned));
      Log.i(TAG, "service up, bridge pinned " + address);
    } else {
      discover = true;
      fallback = withPort(BuildConfig.BRIDGE_HOST);
      String found = prefs.getString(KEY_BRIDGE_FOUND, null);
      if (found != null) {
        setAddress(found);
      }
      Log.i(TAG, "service up, bridge by discovery, else " + (found != null ? found : fallback));
    }
    running = true;
    publish();
    repository.attach(this);
    scheduler.scheduleAtFixedRate(
        this::sampleTrend, TREND_PERIOD_MS, TREND_PERIOD_MS, TimeUnit.MILLISECONDS);
    // Android's shape: the framework says when the link comes and goes (a callback registered
    // while it is already up hears onAvailable shortly), so the poll thread never asks.
    connectivity = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
    connectivity.registerDefaultNetworkCallback(linkWatch);
    new Thread(this::pollLoop, "usage-poll").start();
  }

  /**
   * A host may carry its own port ("192.168.1.5:8790"): a dev PC whose live bridge already owns
   * 8787 runs a demo bridge for the simulator beside it.
   */
  private static String withPort(String host) {
    return host.indexOf(':') >= 0 ? host : host + ":" + UsageFetcher.PORT;
  }

  /** The one write to the two address fields; both are volatile. */
  private void setAddress(String hostPort) {
    address = hostPort;
    url = "http://" + hostPort + "/u";
  }

  @Override
  public int onStartCommand(Intent intent, int flags, int startId) {
    return START_STICKY;
  }

  /** Nothing binds: the screens read the {@link UsageRepository}. */
  @Override
  public IBinder onBind(Intent intent) {
    return null;
  }

  @Override
  public void onDestroy() {
    repository.attach(null);
    scheduler.shutdownNow();
    running = false;
    connectivity.unregisterNetworkCallback(linkWatch);
    synchronized (lock) {
      lock.notifyAll();
    }
    super.onDestroy();
  }

  // ── Requests from the repository (any thread) ──────────────────────────────

  /** X button: fetch now rather than at the next poll. */
  @Override
  public void refreshNow() {
    refreshRequested = true;
    synchronized (lock) {
      lock.notifyAll();
    }
  }

  /** X held: probe the LAN for the bridge before the next fetch, answered or not. */
  @Override
  public void rediscover() {
    rediscoverRequested = true;
    refreshNow();
  }

  // ── Main thread ────────────────────────────────────────────────────────────

  /** Something changed: what is known now, as one new immutable value. */
  private void publish() {
    repository.setData(
        new UsageData(
            snapshot,
            trend,
            linkState,
            linkErr,
            address,
            syncStartedElapsedMs,
            lastGoodElapsedMs,
            lastGoodWallMs,
            nextAttemptElapsedMs));
  }

  /** The trend timer: one sample, published. */
  private void sampleTrend() {
    if (recordTrend()) {
      publish();
    }
  }

  /**
   * Adds one trend sample: the session percent now, or a gap while there are no live numbers.
   * Returns false when there is nothing yet to put a gap in.
   */
  private boolean recordTrend() {
    if (snapshot == null && trend.length == 0) {
      return false;
    }
    boolean fresh = repository.data().getValue().isFresh(SystemClock.elapsedRealtime());
    int keep = trend.length < TREND_SLOTS ? trend.length : TREND_SLOTS - 1;
    int[] next = new int[keep + 1];
    System.arraycopy(trend, trend.length - keep, next, 0, keep);
    next[keep] = fresh ? snapshot.sessionPct : -1;
    trend = next;
    return true;
  }

  /** Epoch ms of 2001-01-01: a wall clock below it has not been anchored this boot. */
  private static final long CLOCK_SET_THRESHOLD_MS = 978_307_200_000L;

  private void applyResult(LinkState state, UsageSnapshot fresh, int retryMs) {
    syncStartedElapsedMs = -1;
    nextAttemptElapsedMs = SystemClock.elapsedRealtime() + retryMs;
    if (state != linkState) {
      Log.i(TAG, "state -> " + state.name());
    }
    linkState = state;
    linkErr = fresh != null ? fresh.err : "";
    if (fresh != null) {
      if (fresh.bridgeEpochS > 0) {
        TimeFormat.setUtcOffsetMinutes(fresh.tzMinutes);
        // The platform's time service anchors the clock from the network
        // (docs/designs/time-service-2026-10.md); the bridge's time is the
        // fallback while that has not happened yet, as on a LAN with no
        // route out. The two agree to within SNTP error once it has.
        long wall = fresh.bridgeEpochS * 1000L;
        if (System.currentTimeMillis() < CLOCK_SET_THRESHOLD_MS) {
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
        if (trend.length == 0 && snapshot.hasLimits()) {
          // The first sample straight away, the rest on the timer.
          trend = new int[] {snapshot.sessionPct};
        }
      }
    }
    // One value for the whole result: the screens repaint once, not once per field.
    publish();
  }

  private void applyLinkOnly(LinkState state) {
    if (state != linkState) {
      Log.i(TAG, "state -> " + state.name());
      linkState = state;
      linkErr = "";
      publish();
    }
  }

  private void applySyncing() {
    syncStartedElapsedMs = SystemClock.elapsedRealtime();
    publish();
  }

  // ── Poll thread ────────────────────────────────────────────────────────────

  private void pollLoop() {
    int failures = 0;
    int unanswered = 0;
    boolean probed = false;
    while (running) {
      // Consumed on every pass, the offline one included: a request left set makes idle() return
      // at once, and while the link is down that spun this loop, flooding the main queue.
      final boolean manual = refreshRequested;
      final boolean again = rediscoverRequested;
      rediscoverRequested = false;
      refreshRequested = false;
      if (!linkUp) {
        // Joining still, or dropped (onLost painted NO_WIFI itself): onAvailable ends the wait.
        if (!everConnected) {
          main.execute(() -> applyLinkOnly(LinkState.JOINING));
        }
        idle(LINK_WAIT_MS);
        continue;
      }
      // Asked at every boot, not only the first: the PC may have a new DHCP lease, and a PC that
      // is up answers in milliseconds.
      if (discover && (again || !probed || (unanswered > 0 && (manual || probeDue(unanswered))))) {
        probed = true;
        probe();
      }
      main.execute(this::applySyncing);

      final UsageSnapshot fresh = new UsageSnapshot();
      final LinkState state = UsageFetcher.fetch(url, fresh);
      final boolean gotReply = state == LinkState.OK || state == LinkState.UPSTREAM;
      failures = state == LinkState.OK ? 0 : failures + 1;
      unanswered = answered(state) ? 0 : unanswered + 1;
      final int wait =
          state == LinkState.OK
              ? POLL_MS
              : (failures <= FAST_RETRIES ? RETRY_FAST_MS : RETRY_SLOW_MS);
      main.execute(() -> applyResult(state, gotReply ? fresh : null, wait));
      idle(wait);
    }
  }

  /** The bridge said something, however wrong: it is at this address. */
  private static boolean answered(LinkState state) {
    return state != LinkState.PC_OFF
        && state != LinkState.BRIDGE_DOWN
        && state != LinkState.NO_REPLY;
  }

  /** One miss is a blip; from the second on, ask the LAN again once a minute. */
  private boolean probeDue(int unanswered) {
    return unanswered >= 2 && SystemClock.elapsedRealtime() - lastProbeElapsedMs >= REDISCOVER_MS;
  }

  /** Broadcast for the bridge; adopt an answer, or the build-time host when there is none yet. */
  private void probe() {
    lastProbeElapsedMs = SystemClock.elapsedRealtime();
    final String found = BridgeDiscovery.find();
    if (found != null) {
      if (!found.equals(address)) {
        setAddress(found);
        prefs.edit().putString(KEY_BRIDGE_FOUND, found).apply();
      }
    } else if (address == null) {
      setAddress(fallback);
    }
  }

  /**
   * Waits up to {@code ms}, cut short by a refresh request, a link change or the Service going
   * away.
   */
  private void idle(int ms) {
    long until = SystemClock.elapsedRealtime() + ms;
    final boolean link = linkUp;
    synchronized (lock) {
      while (running && !refreshRequested && linkUp == link) {
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
