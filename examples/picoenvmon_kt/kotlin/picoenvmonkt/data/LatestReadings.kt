// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.data

import javax.inject.Inject
import javax.inject.Singleton
import picodroid.hardware.Sensor
import picoenvmonkt.IDX_GAS
import picoenvmonkt.IDX_HUMIDITY
import picoenvmonkt.IDX_LIGHT
import picoenvmonkt.IDX_PRESSURE
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.READING_COUNT

/**
 * Latest 1 Hz smoothed value per sensor type — written by `SensorLoggerService`'s smoothing emit,
 * read by the HTTP dashboard (network thread) and the UI. Plain unsynchronized fields: float slots
 * are written whole, and FreeRTOS scheduling on this single-core target makes the cross-thread
 * reads benign (a reader sees either the previous or the current sample, never a torn one — floats
 * occupy one 32-bit slot).
 */
@Singleton
class LatestReadings @Inject constructor() {
    private val values = FloatArray(READING_COUNT)
    private var validMask = 0

    /** Map a [Sensor] type to an index, or -1. */
    private fun indexForType(sensorType: Int): Int =
        when (sensorType) {
            Sensor.TYPE_AMBIENT_TEMPERATURE -> IDX_TEMPERATURE
            Sensor.TYPE_RELATIVE_HUMIDITY -> IDX_HUMIDITY
            Sensor.TYPE_PRESSURE -> IDX_PRESSURE
            Sensor.TYPE_GAS_RESISTANCE -> IDX_GAS
            Sensor.TYPE_LIGHT -> IDX_LIGHT
            else -> -1
        }

    fun updateByType(sensorType: Int, value: Float) {
        val idx = indexForType(sensorType)
        if (idx >= 0) {
            values[idx] = value
            validMask = validMask or (1 shl idx)
        }
    }

    /** Whether `idx` has received at least one sample. */
    fun isValid(idx: Int): Boolean = (validMask and (1 shl idx)) != 0

    fun get(idx: Int): Float = values[idx]
}
