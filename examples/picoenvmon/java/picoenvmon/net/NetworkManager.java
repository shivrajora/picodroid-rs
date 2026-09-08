// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.pm.PackageManager;
import picodroid.net.InetAddress;
import picodroid.net.NetworkInfo;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picoenvmon.EnvApp;
import picoenvmon.data.LatestReadings;
import picoenvmon.util.Formatter;

/**
 * App-scoped owner of everything networked: waits for the WiFi join, then runs the dashboard HTTP
 * server on ONE background thread (each Java thread costs a 16 KB task stack on device). NTP sync
 * and the weather refresh are posted to the framework's shared background pool ({@code
 * Executors.backgroundExecutor()}) so a slow fetch never delays a page: the serve loop's 1 s accept
 * timeout doubles as the housekeeping tick, and the tick only schedules.
 *
 * <p>Fidelity note: Android would host this in a Service. An app-scoped manager is deliberate here
 * — the network stack should outlive Activity churn without a second foreground notification, and
 * the heap budget favors zero extra machinery.
 *
 * <p>State fields are written by the network thread or the pool worker and read by the other
 * threads without synchronization — benign on this single-core cooperative target (every Java task
 * runs at one priority and only switches when it blocks): each field is one 32-bit slot or, for the
 * two due-time longs, tolerant of a torn read (at worst one spare job or one extra tick). Listener
 * callbacks are always posted through {@code Executors.mainExecutor()}, so UI code runs on the main
 * thread only.
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

  /** Initial 30 s wait expired — still retrying at a slow cadence. */
  public static final int STATE_FAILED = 3;

  /** Something to repaint on: network state, time sync, or weather changed. */
  public interface Listener {
    void onNetworkChanged();
  }

  public static final int HTTP_PORT = 8080;

  private static final String TAG = EnvApp.TAG;
  private static final int JOIN_POLL_MS = 500;
  private static final int JOIN_WAIT_LIMIT_MS = 30_000;
  private static final int RETRY_POLL_MS = 5_000;
  private static final int MAX_LISTENERS = 2;

  private final Listener[] listeners = new Listener[MAX_LISTENERS];
  private final LatestReadings latestReadings;
  private final Formatter formatter;

  private static final long NTP_RESYNC_MS = 6L * 3600 * 1000;
  private static final long NTP_RETRY_MS = 5L * 60 * 1000;
  private static final long WEATHER_REFRESH_MS = 15L * 60 * 1000;
  private static final long WEATHER_RETRY_MS = 5L * 60 * 1000;

  /**
   * Ceiling on one background housekeeping job. The worst legitimate job is two DNS lookups (~22 s
   * each on device: FreeRTOS+TCP retries 4 × ~5.5 s), NTP (3 × 3 s) and weather (4 s connect + 4 s
   * read) — about 61 s. If {@link #housekeepingBusy} stays set longer than this, the job was lost
   * and the tick re-arms. Two ways to lose one: {@code BackgroundExecutor.execute} drops silently
   * when its queue is full (only a framework log line says so), and an interpreter-level error
   * (OOM, stack overflow) inside the worker skips {@code finally}.
   */
  private static final long HOUSEKEEPING_STALL_MS = 180_000L;

  private int state = STATE_NO_WIFI;
  private String ipDotted;
  private byte[] ipBytes;
  private String url;
  private String weather;
  private byte[] weatherBytes;
  private boolean started;
  private boolean timeSynced;

  /** Next NTP attempt, on the monotonic elapsed-ms clock. 0 = as soon as the stack is up. */
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

  /** No-op (and stays {@link #STATE_NO_WIFI}) when the board has no WiFi. Idempotent. */
  public void start() {
    if (started) {
      return;
    }
    PackageManager pm = PackageManager.getInstance();
    if (!pm.hasSystemFeature(PackageManager.FEATURE_WIFI)
        && !pm.hasSystemFeature(PackageManager.FEATURE_ETHERNET)) {
      Log.i(TAG, "net: no network link on this board");
      return;
    }
    started = true;
    state = STATE_JOINING;
    new Thread(this).start();
  }

  public int state() {
    return state;
  }

  /** Dotted-quad local address, or null before {@link #STATE_UP}. */
  public String ipAddress() {
    return ipDotted;
  }

  /** Dashboard URL ("http://a.b.c.d:8080/"), or null before {@link #STATE_UP}. */
  public String url() {
    return url;
  }

  /** Whether an SNTP sync has anchored the wall clock this boot. */
  public boolean isTimeSynced() {
    return timeSynced;
  }

  /** Latest weather one-liner, or null (unavailable / not fetched yet). */
  public String weather() {
    return weather;
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

  /** Register for change callbacks (delivered on the main executor). Returns false if full. */
  public boolean addListener(Listener l) {
    for (int i = 0; i < MAX_LISTENERS; i++) {
      if (listeners[i] == null) {
        listeners[i] = l;
        return true;
      }
    }
    return false;
  }

  /** Idempotent. */
  public void removeListener(Listener l) {
    for (int i = 0; i < MAX_LISTENERS; i++) {
      if (listeners[i] == l) {
        listeners[i] = null;
      }
    }
  }

  /** Post one onNetworkChanged round to every listener, on the main thread. */
  void notifyChanged() {
    Executors.mainExecutor()
        .execute(
            () -> {
              for (int i = 0; i < MAX_LISTENERS; i++) {
                Listener l = listeners[i];
                if (l != null) {
                  l.onNetworkChanged();
                }
              }
            });
  }

  // ── Network thread ─────────────────────────────────────────────────────

  @Override
  public void run() {
    waitForNetwork();
    runOnline();
  }

  /**
   * The examples-canonical join wait: hardware needs ~6 s association + ~4 s DHCP, so poll {@code
   * NetworkInfo.isConnected()} rather than racing the boot. After the 30 s budget, drop to a slow
   * retry instead of giving up — WiFi may come back (AP reboot, creds fixed at reflash).
   */
  private void waitForNetwork() {
    int waited = 0;
    while (!NetworkInfo.isConnected()) {
      if (waited >= JOIN_WAIT_LIMIT_MS && state != STATE_FAILED) {
        state = STATE_FAILED;
        Log.i(TAG, "net: still no network after " + (JOIN_WAIT_LIMIT_MS / 1000) + "s");
        notifyChanged();
      }
      int pollMs = state == STATE_FAILED ? RETRY_POLL_MS : JOIN_POLL_MS;
      SystemClock.sleep(pollMs);
      waited += pollMs;
    }
    ipDotted = new InetAddress(NetworkInfo.getIpAddress()).getHostAddress();
    ipBytes = ipDotted.getBytes();
    url = "http://" + ipDotted + ":" + HTTP_PORT + "/";
    state = STATE_UP;
    Log.i(TAG, "net: up, ip=" + ipDotted);
    notifyChanged();
  }

  /**
   * Steady-state loop: serve the dashboard, and let the accept timeout (1 s) double as the
   * housekeeping tick. Bind failures back off rather than kill the thread.
   */
  private void runOnline() {
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
   * still hops to the main executor. The {@code finally} guard pushes any due time still in the
   * past to its retry cadence, so a body that throws before setting one cannot re-post every
   * second.
   */
  private void runHousekeeping() {
    long nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000;
    try {
      if (nowMs >= ntpDueAtMs) {
        boolean ok = SntpClient.sync();
        if (ok != timeSynced) {
          timeSynced = ok;
          notifyChanged();
        }
        ntpDueAtMs = nowMs + (ok ? NTP_RESYNC_MS : NTP_RETRY_MS);
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
        ntpDueAtMs = nowMs + NTP_RETRY_MS;
      }
      if (weatherDueAtMs <= nowMs) {
        weatherDueAtMs = nowMs + WEATHER_RETRY_MS;
      }
      housekeepingBusy = false;
    }
  }

  /** Dashboard footer: clock + address + uptime. */
}
