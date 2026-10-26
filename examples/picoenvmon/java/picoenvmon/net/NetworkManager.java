// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.concurrent.Executors;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;
import picodroid.content.pm.PackageManager;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.net.ConnectivityManager;
import picodroid.net.InetAddress;
import picodroid.net.Network;
import picodroid.net.wifi.WifiManager;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picoenvmon.EnvApp;
import picoenvmon.data.LatestReadings;
import picoenvmon.util.Formatter;

/**
 * App-scoped owner of everything networked: hears the link come and go through a {@link
 * ConnectivityManager.NetworkCallback}, and once it is first up runs the dashboard HTTP server on
 * ONE background thread (each Java thread costs a 16 KB task stack on device). NTP sync and the
 * weather refresh are posted to the framework's shared background pool ({@code
 * Executors.backgroundExecutor()}) so a slow fetch never delays a page: the serve loop's 1 s accept
 * timeout doubles as the housekeeping tick, and the tick only schedules.
 *
 * <p>Fidelity note: Android would host this in a Service. An app-scoped manager is deliberate here
 * — the network stack should outlive Activity churn without a second foreground notification, and
 * the heap budget favors zero extra machinery. The network callback is registered for the life of
 * the process for the same reason: nothing outlives it to unregister.
 *
 * <p>State fields are written by the main thread (the callback), the network thread or the pool
 * worker and read by the others without synchronization — benign on this single-core cooperative
 * target (every Java task runs at one priority and only switches when it blocks): each field is one
 * 32-bit slot or, for the two due-time longs, tolerant of a torn read (at worst one spare job or
 * one extra tick). The UI observes {@link #status()}: set by the callbacks on the main thread,
 * posted to it by the housekeeping worker.
 */
@Singleton
public class NetworkManager implements Runnable {
  /**
   * Board has no network link ({@code FEATURE_WIFI} and {@code FEATURE_ETHERNET} absent) — thread
   * never starts.
   */
  public static final int STATE_NO_WIFI = 0;

  /** Waiting for the join + DHCP (~10 s on hardware; instant in sim). */
  public static final int STATE_JOINING = 1;

  /** IP stack up; server/NTP/weather active. */
  public static final int STATE_UP = 2;

  /** No link 30 s after start — the callback still hears one that comes later. */
  public static final int STATE_FAILED = 3;

  public static final int HTTP_PORT = 8080;

  private static final String TAG = EnvApp.TAG;
  private static final long JOIN_WAIT_LIMIT_MS = 30_000;
  private static final int RETRY_POLL_MS = 5_000;

  private final MutableLiveData<NetworkStatus> status = new MutableLiveData<>();
  private final LatestReadings latestReadings;
  private final Formatter formatter;

  /**
   * The platform's time service anchors the wall clock from the network
   * (docs/designs/time-service-2026-10.md); this job only watches for it, often while the clock is
   * unset (the weather fetch waits on it), rarely once it is.
   */
  private static final long CLOCK_RECHECK_MS = 6L * 3600 * 1000;

  private static final long CLOCK_WAIT_MS = 5_000;

  /** Epoch ms of 2001-01-01: a wall clock below it has not been anchored this boot. */
  private static final long CLOCK_SET_THRESHOLD_MS = 978_307_200_000L;

  private static final long WEATHER_REFRESH_MS = 15L * 60 * 1000;
  private static final long WEATHER_RETRY_MS = 5L * 60 * 1000;

  /**
   * Ceiling on one background housekeeping job. The worst legitimate job is two DNS lookups (~22 s
   * each on device: FreeRTOS+TCP retries 4 × ~5.5 s), NTP (3 × 3 s) and weather (4 s connect, a TLS
   * handshake and the reply under the 4 s read timeout) — about 61 s. If {@link #housekeepingBusy}
   * stays set longer than this, the job was lost and the tick re-arms. Two ways to lose one: {@code
   * BackgroundExecutor.execute} drops silently when its queue is full (only a framework log line
   * says so), and an interpreter-level error (OOM, stack overflow) inside the worker skips {@code
   * finally}.
   */
  private static final long HOUSEKEEPING_STALL_MS = 180_000L;

  private int state = STATE_NO_WIFI;
  private String ipDotted;
  private byte[] ipBytes;
  private String url;
  private String weather;
  private byte[] weatherBytes;
  private boolean started;
  private boolean serving;
  private boolean timeSynced;
  private WifiManager wifiManager;

  /**
   * Next look at the wall clock, on the monotonic elapsed-ms clock. 0 = as soon as the stack is up.
   */
  private long ntpDueAtMs;

  /** Next weather fetch, elapsed-ms clock. 0 = as soon as the stack is up. */
  private long weatherDueAtMs;

  /**
   * True from the serve thread posting a housekeeping job until the worker's {@code finally} clears
   * it. The serve thread reads and sets it with no blocking call in between, so a second job can
   * never be posted on top of a running one.
   */
  private boolean housekeepingBusy;

  /** Elapsed-ms time the in-flight job was posted; the stall-ceiling guard reads it. */
  private long housekeepingPostedAtMs;

  /** Set by the UI's Refresh button; consumed by the next tick that has no job in flight. */
  private boolean refreshRequested;

  @Inject
  public NetworkManager(LatestReadings latestReadings, Formatter formatter) {
    this.latestReadings = latestReadings;
    this.formatter = formatter;
  }

  /** The link as the board's default network: it came up, or it dropped. Main thread. */
  private final ConnectivityManager.NetworkCallback linkCallback =
      new ConnectivityManager.NetworkCallback() {
        @Override
        public void onAvailable(Network network) {
          linkUp();
        }

        @Override
        public void onLost(Network network) {
          linkDown();
        }
      };

  /**
   * Listen for the link and, once it is up, serve the dashboard. No-op (and stays {@link
   * #STATE_NO_WIFI}) when the board has no network link. Idempotent.
   */
  public void start(Context context) {
    if (started) {
      return;
    }
    PackageManager pm = context.getPackageManager();
    if (!pm.hasSystemFeature(PackageManager.FEATURE_WIFI)
        && !pm.hasSystemFeature(PackageManager.FEATURE_ETHERNET)) {
      Log.i(TAG, "net: no network link on this board");
      publish();
      return;
    }
    started = true;
    state = STATE_JOINING;
    publish();
    wifiManager = (WifiManager) context.getSystemService(Context.WIFI_SERVICE);
    ConnectivityManager cm =
        (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
    cm.registerDefaultNetworkCallback(linkCallback);
    // The current state first, then the changes: a link that is up already is served now, not
    // when the callback's first onAvailable gets to the main thread (which a post from
    // Application.onCreate never does here, the main queue not being up yet).
    if (cm.getActiveNetwork() != null) {
      linkUp();
    }
    // Hardware needs ~6 s association + ~4 s DHCP; past 30 s say so, and keep listening — WiFi
    // may come back (AP reboot, creds fixed at reflash).
    Executors.mainScheduledExecutor()
        .schedule(this::joinTimedOut, JOIN_WAIT_LIMIT_MS, TimeUnit.MILLISECONDS);
  }

  /** What the Network screen shows: a new value on every change, on the main thread. */
  public LiveData<NetworkStatus> status() {
    return status;
  }

  /** Whether the platform's time service has anchored the wall clock this boot. */
  boolean isTimeSynced() {
    return timeSynced;
  }

  /**
   * Weather bytes, cached once per 15-min refresh — the serve path must not re-encode the string on
   * every request (its page write is allocation-free).
   */
  byte[] weatherBytes() {
    return weatherBytes;
  }

  /** Dotted-quad IP as bytes, cached at net-up. Null before {@link #STATE_UP}. */
  byte[] ipBytes() {
    return ipBytes;
  }

  /**
   * Ask housekeeping to re-run NTP and weather now. Honored by the next tick once any job already
   * in flight has finished, so a press during a fetch is never lost.
   */
  public void requestRefresh() {
    refreshRequested = true;
  }

  /** Publish the current state to {@link #status()} observers. Main thread. */
  private void publish() {
    status.setValue(snapshot());
  }

  /** As {@link #publish}, from the housekeeping worker: handed to the main thread. */
  private void notifyChanged() {
    status.postValue(snapshot());
  }

  private NetworkStatus snapshot() {
    return new NetworkStatus(state, ipDotted, url, timeSynced, weather);
  }

  // ── Link callbacks (main thread) ───────────────────────────────────────

  /** Idempotent: the point check in {@link #start} and the callback may both report one link. */
  private void linkUp() {
    if (state == STATE_UP) {
      return;
    }
    // The station's address, packed as Android packs it; 0 while it has none.
    int raw = wifiManager.getConnectionInfo().getIpAddress();
    if (raw != 0) {
      ipDotted = new InetAddress(raw).getHostAddress();
      ipBytes = ipDotted.getBytes();
      url = "http://" + ipDotted + ":" + HTTP_PORT + "/";
    }
    state = STATE_UP;
    Log.i(TAG, "net: up, ip=" + ipDotted);
    publish();
    if (!serving) {
      serving = true;
      new Thread(this).start();
    }
  }

  private void linkDown() {
    state = STATE_JOINING;
    ipDotted = null;
    ipBytes = null;
    url = null;
    Log.i(TAG, "net: link lost");
    publish();
  }

  private void joinTimedOut() {
    if (state == STATE_JOINING) {
      state = STATE_FAILED;
      Log.i(TAG, "net: still no network after " + (JOIN_WAIT_LIMIT_MS / 1000) + "s");
      publish();
    }
  }

  // ── Network thread ─────────────────────────────────────────────────────

  /**
   * Steady-state loop, from the first time the link is up: serve the dashboard, and let the accept
   * timeout (1 s) double as the housekeeping tick. Bind failures back off rather than kill the
   * thread.
   */
  @Override
  public void run() {
    HttpServer server = new HttpServer(latestReadings, formatter, this);
    while (true) {
      if (!server.ensureOpen()) {
        SystemClock.sleep(RETRY_POLL_MS);
        continue;
      }
      server.serveOnce();
      scheduleHousekeeping();
    }
  }

  /**
   * Housekeeping tick, on the serve thread about once per second (the accept-timeout tick). Only
   * bookkeeping happens here — it never blocks, so the dashboard keeps answering while a fetch
   * runs. NTP: sync at network-up, re-sync every 6 h, back off 5 min on failure. Weather: refresh
   * every 15 min, same backoff; both fail-soft.
   */
  private void scheduleHousekeeping() {
    long nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000;
    if (housekeepingBusy && nowMs - housekeepingPostedAtMs > HOUSEKEEPING_STALL_MS) {
      Log.i(TAG, "net: housekeeping overdue, re-arming");
      housekeepingBusy = false;
    }
    if (housekeepingBusy) {
      return;
    }
    if (refreshRequested) {
      refreshRequested = false;
      ntpDueAtMs = 0;
      weatherDueAtMs = 0;
    }
    if (nowMs < ntpDueAtMs && nowMs < weatherDueAtMs) {
      return;
    }
    housekeepingBusy = true;
    housekeepingPostedAtMs = nowMs;
    Executors.backgroundExecutor().execute(() -> runHousekeeping());
  }

  /**
   * The blocking half, on a shared {@code jvm-bg} pool worker. Those workers have a small stack (4
   * KB by default; the W board.toml raises it to 6 KB because this job measured 4.7 KB deep), so
   * keep this shallow (no large locals; the fetchers are flat). Blocking here never stalls {@code
   * accept()}. Result fields are single-slot or torn-tolerant (class doc); {@link #notifyChanged}
   * posts to the main thread. The {@code finally} guard pushes any due time still in the past to
   * its retry cadence, so a body that throws before setting one cannot re-post every second.
   */
  private void runHousekeeping() {
    long nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000;
    try {
      if (nowMs >= ntpDueAtMs) {
        boolean ok = System.currentTimeMillis() >= CLOCK_SET_THRESHOLD_MS;
        if (ok != timeSynced) {
          timeSynced = ok;
          Log.i(TAG, ok ? "clock: set by the platform" : "clock: not set");
          notifyChanged();
        }
        ntpDueAtMs = nowMs + (ok ? CLOCK_RECHECK_MS : CLOCK_WAIT_MS);
      }
      if (nowMs >= weatherDueAtMs && !timeSynced) {
        // The HTTPS handshake checks open-meteo's certificate against the
        // wall clock, which the platform's time service anchors once the
        // link is up; the fetch waits for that rather than racing it.
        Log.i(TAG, "weather: waiting for the clock");
        weatherDueAtMs = nowMs + CLOCK_WAIT_MS;
      }
      if (nowMs >= weatherDueAtMs) {
        String w = WeatherFetcher.fetch();
        boolean changed = (w == null) != (weather == null) || (w != null && !w.equals(weather));
        weather = w;
        weatherBytes = w != null ? w.getBytes() : null;
        if (changed) {
          notifyChanged();
        }
        weatherDueAtMs = nowMs + (w != null ? WEATHER_REFRESH_MS : WEATHER_RETRY_MS);
      }
    } catch (RuntimeException e) {
      Log.i(TAG, "net: housekeeping unexpected: " + e);
    } finally {
      if (ntpDueAtMs <= nowMs) {
        ntpDueAtMs = nowMs + CLOCK_WAIT_MS;
      }
      if (weatherDueAtMs <= nowMs) {
        weatherDueAtMs = nowMs + WEATHER_RETRY_MS;
      }
      housekeepingBusy = false;
    }
  }
}
