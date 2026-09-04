// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.net

import javax.inject.Inject
import javax.inject.Singleton
import picodroid.concurrent.Executors
import picodroid.concurrent.Thread
import picodroid.content.pm.PackageManager
import picodroid.net.InetAddress
import picodroid.net.NetworkInfo
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

private const val JOIN_POLL_MS = 500
private const val JOIN_WAIT_LIMIT_MS = 30_000
private const val RETRY_POLL_MS = 5_000
private const val MAX_LISTENERS = 2

private const val NTP_RESYNC_MS = 6L * 3600 * 1000
private const val NTP_RETRY_MS = 5L * 60 * 1000
private const val WEATHER_REFRESH_MS = 15L * 60 * 1000
private const val WEATHER_RETRY_MS = 5L * 60 * 1000

/**
 * App-scoped owner of everything networked: waits for the WiFi join, then runs the dashboard HTTP
 * server, NTP sync, and weather refresh — all on ONE background thread (each Java thread costs a 16
 * KB task stack on device; the serve loop's accept timeout doubles as the housekeeping tick). The
 * thread is `picodroid.concurrent.Thread` — Kotlin default-imports `java.lang.*`, and
 * `java.lang.Thread` does not exist on pico-jvm.
 *
 * Fidelity note: Android would host this in a Service. An app-scoped manager is deliberate here —
 * the network stack should outlive Activity churn without a second foreground notification, and the
 * heap budget favors zero extra machinery.
 *
 * State fields are written by the network thread and read by the UI thread without synchronization
 * — benign on this single-core cooperative target (each field is one 32-bit slot; readers see the
 * previous or current value). Listener callbacks are always posted through
 * `Executors.mainExecutor()`, so UI code runs on the main thread only.
 */
@Singleton
class NetworkManager
@Inject
constructor(private val latestReadings: LatestReadings, private val formatter: Formatter) :
    Runnable {
    /** Something to repaint on: network state, time sync, or weather changed. */
    fun interface Listener {
        fun onNetworkChanged()
    }

    private val listeners = arrayOfNulls<Listener>(MAX_LISTENERS)

    var state = STATE_NO_WIFI
        private set

    /** Dotted-quad local address, or null before [STATE_UP]. */
    var ipAddress: String? = null
        private set

    /** Dotted-quad IP as bytes, cached at net-up. Null before [STATE_UP]. */
    var ipBytes: ByteArray? = null
        private set

    /** Dashboard URL ("http://a.b.c.d:8080/"), or null before [STATE_UP]. */
    var url: String? = null
        private set

    /** Latest weather one-liner, or null (unavailable / not fetched yet). */
    var weather: String? = null
        private set

    /**
     * Weather bytes, cached once per 15-min refresh — the serve path must not re-encode the string
     * on every request (its page write is allocation-free).
     */
    var weatherBytes: ByteArray? = null
        private set

    private var started = false

    /** Whether an SNTP sync has anchored the wall clock this boot. */
    var isTimeSynced = false
        private set

    /** Next NTP attempt, on the monotonic elapsed-ms clock. 0 = as soon as the stack is up. */
    private var ntpDueAtMs = 0L

    /** Next weather fetch, elapsed-ms clock. 0 = as soon as the stack is up. */
    private var weatherDueAtMs = 0L

    /** No-op (and stays [STATE_NO_WIFI]) when the board has no WiFi. Idempotent. */
    fun start() {
        if (started) {
            return
        }
        val pm = PackageManager.getInstance()
        if (
            !pm.hasSystemFeature(PackageManager.FEATURE_WIFI) &&
                !pm.hasSystemFeature(PackageManager.FEATURE_ETHERNET)
        ) {
            Log.i(TAG, "net: no network link on this board")
            return
        }
        started = true
        state = STATE_JOINING
        Thread(this).start()
    }

    /** Ask the housekeeping tick to re-run NTP and weather now. */
    fun requestRefresh() {
        ntpDueAtMs = 0
        weatherDueAtMs = 0
    }

    /** Register for change callbacks (delivered on the main executor). Returns false if full. */
    fun addListener(l: Listener): Boolean {
        for (i in 0 until MAX_LISTENERS) {
            if (listeners[i] == null) {
                listeners[i] = l
                return true
            }
        }
        return false
    }

    /** Idempotent. */
    fun removeListener(l: Listener) {
        for (i in 0 until MAX_LISTENERS) {
            if (listeners[i] === l) {
                listeners[i] = null
            }
        }
    }

    /** Post one onNetworkChanged round to every listener, on the main thread. */
    private fun notifyChanged() {
        Executors.mainExecutor().execute {
            for (i in 0 until MAX_LISTENERS) {
                listeners[i]?.onNetworkChanged()
            }
        }
    }

    // ── Network thread ─────────────────────────────────────────────────────

    override fun run() {
        waitForNetwork()
        runOnline()
    }

    /**
     * The examples-canonical join wait: hardware needs ~6 s association + ~4 s DHCP, so poll
     * `NetworkInfo.isConnected()` rather than racing the boot. After the 30 s budget, drop to a
     * slow retry instead of giving up — WiFi may come back (AP reboot, creds fixed at reflash).
     */
    private fun waitForNetwork() {
        var waited = 0
        while (!NetworkInfo.isConnected()) {
            if (waited >= JOIN_WAIT_LIMIT_MS && state != STATE_FAILED) {
                state = STATE_FAILED
                Log.i(TAG, "net: still no network after ${JOIN_WAIT_LIMIT_MS / 1000}s")
                notifyChanged()
            }
            val pollMs = if (state == STATE_FAILED) RETRY_POLL_MS else JOIN_POLL_MS
            SystemClock.sleep(pollMs)
            waited += pollMs
        }
        val ip = InetAddress(NetworkInfo.getIpAddress()).hostAddress
        ipAddress = ip
        ipBytes = formatter.ascii(ip)
        url = "http://$ip:$HTTP_PORT/"
        state = STATE_UP
        Log.i(TAG, "net: up, ip=$ip")
        notifyChanged()
    }

    /**
     * Steady-state loop: serve the dashboard, and let the accept timeout (1 s) double as the
     * housekeeping tick. Bind failures back off rather than kill the thread.
     */
    private fun runOnline() {
        val server = HttpServer(latestReadings, formatter, this)
        while (true) {
            if (!server.ensureOpen()) {
                SystemClock.sleep(RETRY_POLL_MS)
                continue
            }
            server.serveOnce()
            housekeeping()
        }
    }

    /**
     * Periodic work between serves (runs about once per second, on the accept-timeout tick). NTP:
     * sync at network-up, re-sync every 6 h, back off 5 min on failure. Weather: refresh every 15
     * min, same backoff; both fail-soft.
     */
    private fun housekeeping() {
        val nowMs = SystemClock.elapsedRealtimeNanos() / 1_000_000
        if (nowMs >= ntpDueAtMs) {
            val ok = sntpSync()
            if (ok != isTimeSynced) {
                isTimeSynced = ok
                notifyChanged()
            }
            ntpDueAtMs = nowMs + (if (ok) NTP_RESYNC_MS else NTP_RETRY_MS)
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
    }
}
