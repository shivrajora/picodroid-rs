// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.service

import javax.inject.Inject
import picodroid.app.Notification
import picodroid.app.Service
import picodroid.content.Intent
import picodroid.hardware.Sensor
import picodroid.hardware.SensorEvent
import picodroid.hardware.SensorEventListener
import picodroid.hardware.SensorManager
import picodroid.os.IBinder
import picodroid.os.SystemClock
import picodroid.util.Log
import picoenvmonkt.IDX_GAS
import picoenvmonkt.IDX_HUMIDITY
import picoenvmonkt.IDX_LIGHT
import picoenvmonkt.IDX_PRESSURE
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.READING_COUNT
import picoenvmonkt.RING_CAPACITY
import picoenvmonkt.data.LatestReadings
import picoenvmonkt.data.SensorRingBuffer
import picoenvmonkt.data.ThresholdConfig
import picoenvmonkt.hardware.RgbLed
import picoenvmonkt.util.Formatter
import picoenvmonkt.util.hms

private const val TAG = "SensorLoggerKt"
private const val NOTIFICATION_ID = 1

// ── 1 Hz smoothing ───────────────────────────────────────────────────────────
// Per-type windowed-mean accumulators. Raw callbacks arrive ~5 Hz in a single
// burst; on the first callback past the 1 s mark we emit averages to every
// registered SmoothedSensorListener and reset the accumulators.
private const val EMIT_INTERVAL_MS = 1000L
private const val MAX_SMOOTHED_LISTENERS = 4

// ── Alert edge detection ─────────────────────────────────────────────────────
// Raw callbacks re-evaluate thresholds several times per second; logging every
// breached sample flooded the log and allocated a concat per line forever
// (~13 allocs/s at idle with default thresholds indoors). Latch per sensor and
// log only the transitions: once entering breach, once clearing. The latches
// live on the instance, which survives bind/unbind churn; a service restart
// re-logs at most one active breach per sensor.
private const val ALERT_TEMP = 0
private const val ALERT_HUMIDITY = 1
private const val ALERT_LIGHT = 2

/**
 * Foreground Service that streams every sensor reading into a per-type ring buffer, drives the
 * Pack's RGB LED from gas IAQ, and logs threshold breaches. Bind via `bindService` to read the ring
 * buffers from a UI activity.
 */
class SensorLoggerService : Service(), SensorEventListener {
    /**
     * Nested (static) by default in Kotlin — an `inner class` would be rejected by the DI check.
     */
    class LocalBinder : IBinder {
        @JvmField var service: SensorLoggerService? = null
    }

    private val binder = LocalBinder()
    private val rings =
        arrayOf(
            SensorRingBuffer(RING_CAPACITY),
            SensorRingBuffer(RING_CAPACITY),
            SensorRingBuffer(RING_CAPACITY),
            SensorRingBuffer(RING_CAPACITY),
            SensorRingBuffer(RING_CAPACITY),
        )

    private var sensorManager: SensorManager? = null
    @Inject lateinit var rgbLed: RgbLed
    @Inject lateinit var thresholds: ThresholdConfig
    @Inject lateinit var latestReadings: LatestReadings
    @Inject lateinit var formatter: Formatter

    /**
     * Whether the foreground logging pass is currently active — i.e. `startService` has run and
     * `stopService` has not. A started Service outlives the unbind that happens when the Live
     * screen is left, so a re-entering UI queries this to restore the Logger toggle to its true
     * state.
     */
    var isLogging = false
        private set

    /** Index i of every per-type table is the sensor type at `smoothedTypes[i]`. */
    private val smoothedTypes =
        intArrayOf(
            Sensor.TYPE_AMBIENT_TEMPERATURE,
            Sensor.TYPE_RELATIVE_HUMIDITY,
            Sensor.TYPE_PRESSURE,
            Sensor.TYPE_GAS_RESISTANCE,
            Sensor.TYPE_LIGHT,
        )
    private val smoothSum = FloatArray(READING_COUNT)
    private val smoothCount = IntArray(READING_COUNT)
    private val smoothedListeners = arrayOfNulls<SmoothedSensorListener>(MAX_SMOOTHED_LISTENERS)
    private var lastEmitMs = 0L

    private val alertActive = BooleanArray(3)

    override fun onCreate() {
        binder.service = this
        val mgr = SensorManager.getInstance()
        sensorManager = mgr
        registerAll(mgr)
        Log.i(TAG, "onCreate")
    }

    private fun registerAll(mgr: SensorManager) {
        for (t in smoothedTypes) {
            val s = mgr.getDefaultSensor(t)
            if (s != null) {
                mgr.registerListener(this, s, SensorManager.SENSOR_DELAY_NORMAL)
            } else {
                Log.i(TAG, "no default sensor for type=$t")
            }
        }
    }

    override fun onStartCommand(intent: Intent, flags: Int, startId: Int): Int {
        if (!isLogging) {
            isLogging = true
            val n =
                Notification.Builder()
                    .setContentTitle("PicoEnvMonKt")
                    .setContentText("Logging sensors")
                    .build()
            startForeground(NOTIFICATION_ID, n)
            Log.i(TAG, "foreground started")
        }
        return Service.START_STICKY
    }

    override fun onBind(intent: Intent): IBinder = binder

    override fun onDestroy() {
        Log.i(TAG, "onDestroy")
        sensorManager?.unregisterListener(this)
        rgbLed.off()
        stopForeground(true)
    }

    override fun onSensorChanged(event: SensorEvent) {
        val type = event.sensor.type
        val v = event.values[0]
        val ring = ringFor(type)
        if (ring != null) {
            // currentTimeMillis is epoch time only after the NTP sync anchors it;
            // before that it counts from boot, which the >100e9 ms (~1973) sanity
            // bound filters to ts=0 ("unknown") without coupling to NetworkManager.
            val now = System.currentTimeMillis()
            val epochSec = if (now > 100_000_000_000L) (now / 1000).toInt() else 0
            ring.add(v, epochSec)
        }

        when (type) {
            Sensor.TYPE_GAS_RESISTANCE -> applyLedFromIaq(v)
            Sensor.TYPE_AMBIENT_TEMPERATURE ->
                alertEdge(ALERT_TEMP, thresholds.tempBreached(v), "temperature breach", v, " C")
            Sensor.TYPE_RELATIVE_HUMIDITY ->
                alertEdge(
                    ALERT_HUMIDITY,
                    thresholds.humidityBreached(v),
                    "humidity below threshold",
                    v,
                    " m%",
                )
            Sensor.TYPE_LIGHT ->
                alertEdge(ALERT_LIGHT, thresholds.luxBreached(v), "light below threshold", v, " lx")
            else -> {}
        }

        val smIdx = smoothedIdxFor(type)
        if (smIdx >= 0) {
            smoothSum[smIdx] += v
            smoothCount[smIdx]++
            // Monotonic clock for the interval: currentTimeMillis jumps when the
            // NTP sync anchors the wall clock, which must not stall or burst the
            // 1 Hz emit cadence.
            val now = SystemClock.elapsedRealtimeNanos() / 1_000_000
            if (lastEmitMs == 0L) {
                lastEmitMs = now
            }
            if (now - lastEmitMs >= EMIT_INTERVAL_MS) {
                emitSmoothed()
                lastEmitMs = now
            }
        }
    }

    /**
     * Log threshold alerts only on state transitions: one line on entering breach, one on clearing.
     */
    private fun alertEdge(idx: Int, breached: Boolean, what: String, v: Float, unit: String) {
        if (breached && !alertActive[idx]) {
            alertActive[idx] = true
            Log.i(TAG, "ALERT${alertStamp()}: $what: $v$unit")
        } else if (!breached && alertActive[idx]) {
            alertActive[idx] = false
            Log.i(TAG, "ALERT cleared${alertStamp()}: $what: $v$unit")
        }
    }

    /** " [HH:MM:SS]" once the wall clock is NTP-anchored, "" before. Edge-latched — no churn. */
    private fun alertStamp(): String {
        val now = System.currentTimeMillis()
        return if (now > 100_000_000_000L) " [${hms(now)}]" else ""
    }

    /** Register for 1 Hz windowed-mean callbacks. Returns false if all slots are full. */
    fun addSmoothedListener(l: SmoothedSensorListener): Boolean {
        for (i in 0 until MAX_SMOOTHED_LISTENERS) {
            if (smoothedListeners[i] == null) {
                smoothedListeners[i] = l
                return true
            }
        }
        return false
    }

    /** Idempotent: removing an unregistered listener is a no-op. */
    fun removeSmoothedListener(l: SmoothedSensorListener) {
        for (i in 0 until MAX_SMOOTHED_LISTENERS) {
            if (smoothedListeners[i] === l) {
                smoothedListeners[i] = null
            }
        }
    }

    /** A written-out scan: `IntArray.indexOf` is a `kotlin/collections/ArraysKt` call. */
    private fun smoothedIdxFor(sensorType: Int): Int {
        for (i in 0 until READING_COUNT) {
            if (smoothedTypes[i] == sensorType) {
                return i
            }
        }
        return -1
    }

    private fun emitSmoothed() {
        for (i in 0 until READING_COUNT) {
            if (smoothCount[i] == 0) {
                continue
            }
            val avg = smoothSum[i] / smoothCount[i]
            smoothSum[i] = 0f
            smoothCount[i] = 0
            latestReadings.updateByType(smoothedTypes[i], avg)
            for (j in 0 until MAX_SMOOTHED_LISTENERS) {
                smoothedListeners[j]?.onSmoothedSensor(smoothedTypes[i], avg)
            }
        }
    }

    override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {}

    private fun applyLedFromIaq(gasOhm: Float) {
        val iaq = formatter.iaqFromGas(gasOhm)
        // 0 (clean) → green; 250 → yellow; 500 → red.
        val r: Int
        var g: Int
        if (iaq < 250) {
            r = (iaq * 255f / 250f).toInt()
            g = 255
        } else {
            r = 255
            g = ((500 - iaq) * 255f / 250f).toInt()
            if (g < 0) {
                g = 0
            }
        }
        rgbLed.setColor(r, g, 0)
    }

    private fun ringFor(type: Int): SensorRingBuffer? =
        when (type) {
            Sensor.TYPE_AMBIENT_TEMPERATURE -> rings[IDX_TEMPERATURE]
            Sensor.TYPE_RELATIVE_HUMIDITY -> rings[IDX_HUMIDITY]
            Sensor.TYPE_PRESSURE -> rings[IDX_PRESSURE]
            Sensor.TYPE_GAS_RESISTANCE -> rings[IDX_GAS]
            Sensor.TYPE_LIGHT -> rings[IDX_LIGHT]
            else -> null
        }

    /** Snapshot of one ring buffer. `idx` = one of `IDX_TEMPERATURE` … */
    fun snapshot(idx: Int, out: FloatArray): Int {
        if (idx < 0 || idx >= rings.size) {
            return 0
        }
        return rings[idx].snapshot(out)
    }

    /** As [snapshot], also copying per-sample epoch-second timestamps. */
    fun snapshot(idx: Int, out: FloatArray, tsOut: IntArray): Int {
        if (idx < 0 || idx >= rings.size) {
            return 0
        }
        return rings[idx].snapshot(out, tsOut)
    }
}
