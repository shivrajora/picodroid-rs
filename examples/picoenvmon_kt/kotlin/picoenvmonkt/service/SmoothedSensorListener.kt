// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.service

/**
 * Receives 1 Hz windowed-mean sensor values from [SensorLoggerService]. Lets a UI consumer (e.g.
 * `LiveActivity`) avoid registering its own 5 Hz `SensorEventListener` and instead get one calm
 * callback per sensor type per second.
 */
fun interface SmoothedSensorListener {
    /** sensorType is one of `Sensor.TYPE_*` — same constants the raw API uses. */
    fun onSmoothedSensor(sensorType: Int, value: Float)
}
