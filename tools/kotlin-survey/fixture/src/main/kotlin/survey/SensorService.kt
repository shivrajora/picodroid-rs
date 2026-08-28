// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.app.Service
import picodroid.content.Intent
import picodroid.hardware.Sensor
import picodroid.hardware.SensorEvent
import picodroid.hardware.SensorEventListener
import picodroid.hardware.SensorManager
import picodroid.os.IBinder
import picodroid.util.Log

/**
 * SensorLoggerService shape: a Service that is its own SensorEventListener,
 * an inner LocalBinder, a `fun interface` listener, `Array(n) { }`, `IntArray`,
 * `when` over SDK `static final int` constants, and an `@Synchronized` method.
 */
class SensorService : Service(), SensorEventListener {
    fun interface SmoothedListener {
        fun onSmoothed(type: Int, value: Float)
    }

    inner class LocalBinder : IBinder {
        val service: SensorService
            get() = this@SensorService
    }

    private val binder = LocalBinder()
    private val buffers = Array(KINDS) { RingBuffer(60) }
    private val counts = IntArray(KINDS)
    private val listeners = mutableListOf<SmoothedListener>()
    private var logging = false
    private var latestValue = 0f

    override fun onCreate() {
        super.onCreate()
        val sm = SensorManager.getInstance()
        sm.getDefaultSensor(Sensor.TYPE_LIGHT)?.let { sm.registerListener(this, it, SensorManager.SENSOR_DELAY_NORMAL) }
        addListener { t, v -> Log.i(TAG, "$t=$v") }
    }

    override fun onStartCommand(intent: Intent?, startId: Int): Int {
        Log.d(TAG, "start $startId from ${intent?.targetClassName}")
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onDestroy() {
        SensorManager.getInstance().unregisterListener(this)
        super.onDestroy()
    }

    override fun onSensorChanged(event: SensorEvent) {
        val v = event.values[0]
        val slot = when (event.sensor.type) {
            Sensor.TYPE_LIGHT -> 0
            Sensor.TYPE_PRESSURE -> 1
            Sensor.TYPE_RELATIVE_HUMIDITY -> 2
            Sensor.TYPE_AMBIENT_TEMPERATURE -> 3
            else -> return
        }
        buffers[slot].add(v, (event.timestamp / 1_000_000_000L).toInt())
        counts[slot]++
        latestValue = v
        if (logging) Log.d(TAG, "sensor $slot=$v")
        listeners.forEach { it.onSmoothed(slot, buffers[slot].average()) }
    }

    override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {
        Log.d(TAG, "accuracy ${sensor.name}=$accuracy")
    }

    fun addListener(l: SmoothedListener) {
        listeners += l
    }

    @Synchronized
    fun latest(): Float = latestValue

    fun setLogging(on: Boolean) {
        logging = on
    }

    fun describe(): String = "counts=${counts.sum()} max=${buffers[0].max()} spread=${buffers[1].spread()}"

    companion object {
        const val TAG = "SensorService"
        const val KINDS = 4
    }
}
