// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.net

import javax.inject.Inject
import javax.inject.Singleton
import picodroid.concurrent.Executors
import picodroid.concurrent.TimeUnit
import picodroid.content.Context
import picodroid.content.pm.PackageManager
import picodroid.lifecycle.LiveData
import picodroid.lifecycle.MutableLiveData
import picodroid.net.ConnectivityManager
import picodroid.net.InetAddress
import picodroid.net.Network
import picodroid.net.wifi.WifiManager
import picodroid.os.SystemClock
import picodroid.util.Log
import picoenvmonkt.HTTP_PORT
import picoenvmonkt.STATE_FAILED
import picoenvmonkt.STATE_JOINING
import picoenvmonkt.STATE_NO_WIFI
import picoenvmonkt.STATE_UP
import picoenvmonkt.TAG
import picoenvmonkt.data.LatestReadings
import picoenvmonkt.util.Formatter

private const val JOIN_WAIT_LIMIT_MS = 30_000L
private const val RETRY_POLL_MS = 5_000

/**
 * The platform's time service anchors the wall clock from the network
 * (docs/designs/time-service-2026-10.md); this job only watches for it, often while the clock is
 * unset (the weather fetch waits on it), rarely once it is.
 */
private const val CLOCK_RECHECK_MS = 6L * 3600 * 1000
private const val CLOCK_WAIT_MS = 5_000L

/** Epoch ms of 2001-01-01: a wall clock below it has not been anchored this boot. */
private const val CLOCK_SET_THRESHOLD_MS = 978_307_200_000L
private const val WEATHER_REFRESH_MS = 15L * 60 * 1000
private const val WEATHER_RETRY_MS = 5L * 60 * 1000

/**
 * Ceiling on one background housekeeping job. The worst legitimate job is two DNS lookups (~22 s
 * each on device: FreeRTOS+TCP retries 4 × ~5.5 s), NTP (3 × 3 s) and weather (4 s connect, a TLS
 * handshake and the reply under the 4 s read timeout) — about 61 s. If `housekeepingBusy` stays set
 * longer than this, the job was lost and the tick re-arms. Two ways to lose one:
 * `BackgroundExecutor.execute` drops silently when its queue is full (only a framework log line
 * says so), and an interpreter-level error (OOM, stack overflow) inside the worker skips `finally`.
 */
private const val HOUSEKEEPING_STALL_MS = 180_000L

/**
 * App-scoped owner of everything networked: hears the link come and go through a
 * [ConnectivityManager.NetworkCallback], and once it is first up runs the dashboard HTTP server on
 * ONE background thread (each Java thread costs a 16 KB task stack on device). NTP sync and the
 * weather refresh are posted to the framework's shared background pool
 * (`Executors.backgroundExecutor()`) so a slow fetch never delays a page: the serve loop's 1 s
 * accept timeout doubles as the housekeeping tick, and the tick only schedules.
 *
 * Fidelity note: Android would host this in a Service. An app-scoped manager is deliberate here —
 * the network stack should outlive Activity churn without a second foreground notification, and the
 * heap budget favors zero extra machinery. The network callback is registered for the life of the
 * process for the same reason: nothing outlives it to unregister.
 *
 * State fields are written by the main thread (the callback), the network thread or the pool worker
 * and read by the others without synchronization — benign on this single-core cooperative target
 * (every Java task runs at one priority and only switches when it blocks): each field is one 32-bit
 * slot or, for the two due-time longs, tolerant of a torn read (at worst one spare job or one extra
 * tick). The UI observes [status]: set by the callbacks on the main thread, posted to it by the
 * housekeeping worker.
 */
@Singleton
class NetworkManager
@Inject
constructor(private val latestReadings: LatestReadings, private val formatter: Formatter) :
    Runnable {
    private val status = MutableLiveData<NetworkStatus>()

    private var state = STATE_NO_WIFI

    /** Dotted-quad local address, or null while the link is down or its address unknown. */
    private var ipAddress: String? = null

    /** Dotted-quad IP as bytes, cached at net-up. Null before [STATE_UP]. */
    var ipBytes: ByteArray? = null
        private set

    /** Dashboard URL ("http://a.b.c.d:8080/"), or null with [ipAddress]. */
    private var url: String? = null

    /** Latest weather one-liner, or null (unavailable / not fetched yet). */
    private var weather: String? = null

    /**
     * Weather bytes, cached once per 15-min refresh — the serve path must not re-encode the string
     * on every request (its page write is allocation-free).
     */
    var weatherBytes: ByteArray? = null
        private set

    private var started = false
    private var serving = false
    private var wifiManager: WifiManager? = null

    /** Whether an SNTP sync has anchored the wall clock this boot. */
    var isTimeSynced = false
        private set

    /** Next NTP attempt, on the monotonic elapsed-ms clock. 0 = as soon as the stack is up. */
    private var ntpDueAtMs = 0L

    /** Next weather fetch, elapsed-ms clock. 0 = as soon as the stack is up. */
    private var weatherDueAtMs = 0L

    /**
     * True from the serve thread posting a housekeeping job until the worker's `finally` clears it.
     * The serve thread reads and sets it with no blocking call in between, so a second job can
     * never be posted on top of a running one.
     */
    private var housekeepingBusy = false

    /** Elapsed-ms time the in-flight job was posted; the stall-ceiling guard reads it. */
    private var housekeepingPostedAtMs = 0L

    /** Set by the UI's Refresh button; consumed by the next tick that has no job in flight. */
    private var refreshRequested = false

    /** The link as the board's default network: it came up, or it dropped. Main thread. */
    private val linkCallback =
        object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                linkUp()
            }

            override fun onLost(network: Network) {
                linkDown()
            }
        }

    /**
     * Listen for the link and, once it is up, serve the dashboard. No-op (and stays
     * [STATE_NO_WIFI]) when the board has no network link. Idempotent.
     */
    fun start(context: Context) {
        if (started) {
            return
        }
        val pm = context.getPackageManager()
        if (
            !pm.hasSystemFeature(PackageManager.FEATURE_WIFI) &&
                !pm.hasSystemFeature(PackageManager.FEATURE_ETHERNET)
        ) {
            Log.i(TAG, "net: no network link on this board")
            publish()
            return
        }
        started = true
        state = STATE_JOINING
        publish()
        wifiManager = context.getSystemService(Context.WIFI_SERVICE) as WifiManager
        val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        cm.registerDefaultNetworkCallback(linkCallback)
        // The current state first, then the changes: a link that is up already is served now, not
        // when the callback's first onAvailable gets to the main thread (which a post from
        // Application.onCreate never does here, the main queue not being up yet).
        if (cm.getActiveNetwork() != null) {
            linkUp()
        }
        // Hardware needs ~6 s association + ~4 s DHCP; past 30 s say so, and keep listening —
        // WiFi may come back (AP reboot, creds fixed at reflash).
        Executors.mainScheduledExecutor()
            .schedule(Runnable { joinTimedOut() }, JOIN_WAIT_LIMIT_MS, TimeUnit.MILLISECONDS)
    }

    /** What the Network screen shows: a new value on every change, on the main thread. */
    fun status(): LiveData<NetworkStatus> = status

    /**
     * Ask housekeeping to re-run NTP and weather now. Honored by the next tick once any job already
     * in flight has finished, so a press during a fetch is never lost.
     */
    fun requestRefresh() {
        refreshRequested = true
    }

    /** Publish the current state to [status] observers. Main thread. */
    private fun publish() {
        status.setValue(snapshot())
    }

    /** As [publish], from the housekeeping worker: handed to the main thread. */
    private fun notifyChanged() {
        status.postValue(snapshot())
    }

    private fun snapshot(): NetworkStatus =
        NetworkStatus(state, ipAddress, url, isTimeSynced, weather)

    // ── Link callbacks (main thread) ───────────────────────────────────────

    /** Idempotent: the point check in [start] and the callback may both report one link. */
    private fun linkUp() {
        if (state == STATE_UP) {
            return
        }
        // The station's address, packed as Android packs it; 0 while it has none.
        val raw = wifiManager?.getConnectionInfo()?.getIpAddress() ?: 0
        if (raw != 0) {
            val ip = InetAddress(raw).hostAddress
            ipAddress = ip
            ipBytes = formatter.ascii(ip)
            url = "http://$ip:$HTTP_PORT/"
        }
        state = STATE_UP
        Log.i(TAG, "net: up, ip=$ipAddress")
        publish()
        if (!serving) {
            serving = true
            Thread(this).start()
        }
    }

    private fun linkDown() {
        state = STATE_JOINING
        ipAddress = null
        ipBytes = null
        url = null
        Log.i(TAG, "net: link lost")
        publish()
    }

    private fun joinTimedOut() {
        if (state == STATE_JOINING) {
            state = STATE_FAILED
            Log.i(TAG, "net: still no network after ${JOIN_WAIT_LIMIT_MS / 1000}s")
            publish()
        }
    }

    // ── Network thread ─────────────────────────────────────────────────────

    /**
     * Steady-state loop, from the first time the link is up: serve the dashboard, and let the
     * accept timeout (1 s) double as the housekeeping tick. Bind failures back off rather than kill
     * the thread.
     */
    override fun run() {
        val server = HttpServer(latestReadings, formatter, this)
        while (true) {
            if (!server.ensureOpen()) {
                SystemClock.sleep(RETRY_POLL_MS)
                continue
            }
            server.serveOnce()
            scheduleHousekeeping()
        }
    }

    /**
     * Housekeeping tick, on the serve thread about once per second (the accept-timeout tick). Only
     * bookkeeping happens here — it never blocks, so the dashboard keeps answering while a fetch
     * runs. NTP: sync at network-up, re-sync every 6 h, back off 5 min on failure. Weather: refresh
     * every 15 min, same backoff; both fail-soft.
     */
    private fun scheduleHousekeeping() {
        val nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000
        if (housekeepingBusy && nowMs - housekeepingPostedAtMs > HOUSEKEEPING_STALL_MS) {
            Log.i(TAG, "net: housekeeping overdue, re-arming")
            housekeepingBusy = false
        }
        if (housekeepingBusy) {
            return
        }
        if (refreshRequested) {
            refreshRequested = false
            ntpDueAtMs = 0
            weatherDueAtMs = 0
        }
        if (nowMs < ntpDueAtMs && nowMs < weatherDueAtMs) {
            return
        }
        housekeepingBusy = true
        housekeepingPostedAtMs = nowMs
        Executors.backgroundExecutor().execute { runHousekeeping() }
    }

    /**
     * The blocking half, on a shared `jvm-bg` pool worker. Those workers have a small stack (4 KB
     * by default; the W board.toml raises it to 6 KB because this job measured 4.7 KB deep), so
     * keep this shallow (no large locals; the fetchers are flat). Blocking here never stalls
     * `accept()`. Result fields are single-slot or torn-tolerant (class doc); [notifyChanged] posts
     * to the main thread. The `finally` guard pushes any due time still in the past to its retry
     * cadence, so a body that throws before setting one cannot re-post every second.
     */
    private fun runHousekeeping() {
        val nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000
        try {
            if (nowMs >= ntpDueAtMs) {
                val ok = System.currentTimeMillis() >= CLOCK_SET_THRESHOLD_MS
                if (ok != isTimeSynced) {
                    isTimeSynced = ok
                    Log.i(TAG, if (ok) "clock: set by the platform" else "clock: not set")
                    notifyChanged()
                }
                ntpDueAtMs = nowMs + (if (ok) CLOCK_RECHECK_MS else CLOCK_WAIT_MS)
            }
            if (nowMs >= weatherDueAtMs && !isTimeSynced) {
                // The HTTPS handshake checks open-meteo's certificate against the wall clock,
                // which the platform's time service anchors once the link is up; the fetch
                // waits for that rather than racing it.
                Log.i(TAG, "weather: waiting for the clock")
                weatherDueAtMs = nowMs + CLOCK_WAIT_MS
            }
            if (nowMs >= weatherDueAtMs) {
                val w = fetchWeather()
                val changed = (w == null) != (weather == null) || (w != null && w != weather)
                weather = w
                weatherBytes = if (w != null) formatter.ascii(w) else null
                if (changed) {
                    notifyChanged()
                }
                weatherDueAtMs = nowMs + (if (w != null) WEATHER_REFRESH_MS else WEATHER_RETRY_MS)
            }
        } catch (e: RuntimeException) {
            Log.i(TAG, "net: housekeeping unexpected: $e")
        } finally {
            if (ntpDueAtMs <= nowMs) {
                ntpDueAtMs = nowMs + CLOCK_WAIT_MS
            }
            if (weatherDueAtMs <= nowMs) {
                weatherDueAtMs = nowMs + WEATHER_RETRY_MS
            }
            housekeepingBusy = false
        }
    }
}
