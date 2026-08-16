// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.pm.PackageManager;
import picodroid.net.InetAddress;
import picodroid.net.NetworkInfo;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picoenvmon.di.EnvAppComponent;

/**
 * App-scoped owner of everything networked: waits for the WiFi join, then runs the dashboard HTTP
 * server, NTP sync, and weather refresh — all on ONE background thread (each Java thread costs a 16
 * KB task stack on device; the serve loop's accept timeout doubles as the housekeeping tick).
 *
 * <p>Fidelity note: Android would host this in a Service. An app-scoped manager is deliberate here
 * — the network stack should outlive Activity churn without a second foreground notification, and
 * the heap budget favors zero extra machinery.
 *
 * <p>State fields are written by the network thread and read by the UI thread without
 * synchronization — benign on this single-core cooperative target (each field is one 32-bit slot;
 * readers see the previous or current value). Listener callbacks are always posted through {@code
 * Executors.mainExecutor()}, so UI code runs on the main thread only.
 */
public class NetworkManager implements Runnable {
  /** Board has no WiFi hardware ({@code FEATURE_WIFI} absent) — thread never starts. */
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

  private static final String TAG = EnvAppComponent.TAG;
  private static final int JOIN_POLL_MS = 500;
  private static final int JOIN_WAIT_LIMIT_MS = 30_000;
  private static final int RETRY_POLL_MS = 5_000;
  private static final int MAX_LISTENERS = 2;

  private final Listener[] listeners = new Listener[MAX_LISTENERS];

  private static final long NTP_RESYNC_MS = 6L * 3600 * 1000;
  private static final long NTP_RETRY_MS = 5L * 60 * 1000;
  private static final long WEATHER_REFRESH_MS = 15L * 60 * 1000;
  private static final long WEATHER_RETRY_MS = 5L * 60 * 1000;

  private int state = STATE_NO_WIFI;
  private String ipDotted;
  private String url;
  private String weather;
  private boolean started;
  private boolean timeSynced;

  /** Next NTP attempt, on the monotonic elapsed-ms clock. 0 = as soon as the stack is up. */
  private long ntpDueAtMs;

  /** Next weather fetch, elapsed-ms clock. 0 = as soon as the stack is up. */
  private long weatherDueAtMs;

  /** No-op (and stays {@link #STATE_NO_WIFI}) when the board has no WiFi. Idempotent. */
  public void start() {
    if (started) {
      return;
    }
    if (!PackageManager.getInstance().hasSystemFeature(PackageManager.FEATURE_WIFI)) {
      Log.i(TAG, "net: no WiFi on this board");
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

  /** Ask the housekeeping tick to re-run NTP and weather now. */
  public void requestRefresh() {
    ntpDueAtMs = 0;
    weatherDueAtMs = 0;
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
    HttpServer server = new HttpServer((EnvAppComponent) EnvAppComponent.current(), this);
    while (true) {
      if (!server.ensureOpen()) {
        SystemClock.sleep(RETRY_POLL_MS);
        continue;
      }
      server.serveOnce();
      housekeeping();
    }
  }

  /**
   * Periodic work between serves (runs about once per second, on the accept-timeout tick). NTP:
   * sync at network-up, re-sync every 6 h, back off 5 min on failure. Weather: refresh every 15
   * min, same backoff; both fail-soft.
   */
  private void housekeeping() {
    long nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000;
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
      if (changed) {
        notifyChanged();
      }
      weatherDueAtMs = nowMs + (w != null ? WEATHER_REFRESH_MS : WEATHER_RETRY_MS);
    }
  }

  /** Dashboard footer: clock + address + uptime. */
  String statusFooter() {
    String time =
        timeSynced
            ? picoenvmon.util.TimeFormat.hms(System.currentTimeMillis()) + " UTC - "
            : "time not synced - ";
    return time + "IP " + ipDotted + " - up " + HttpServer.uptime();
  }
}
