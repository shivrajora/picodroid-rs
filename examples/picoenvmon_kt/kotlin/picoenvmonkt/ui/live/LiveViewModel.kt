// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.live

import javax.inject.Inject
import picodroid.lifecycle.LiveData
import picodroid.lifecycle.Transformations
import picodroid.lifecycle.ViewModel
import picodroid.lifecycle.ViewModelProvider
import picoenvmonkt.IDX_GAS
import picoenvmonkt.IDX_HUMIDITY
import picoenvmonkt.IDX_LIGHT
import picoenvmonkt.IDX_PRESSURE
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.READING_COUNT
import picoenvmonkt.data.LatestReadings
import picoenvmonkt.data.Readings
import picoenvmonkt.data.ThresholdConfig
import picoenvmonkt.util.Formatter

/**
 * What the Live screen shows: the repository's 1 Hz readings turned into tile text and breach flags
 * in the user's units, so the Activity only paints. Made by its [Factory], which the object graph
 * injects into the Activity with the singletons it needs.
 */
class LiveViewModel
internal constructor(
    readings: LatestReadings,
    private val formatter: Formatter,
    private val thresholds: ThresholdConfig,
) : ViewModel() {

    /** One frame of the five tiles, indexed like `IDX_TEMPERATURE`. */
    class Tiles {
        /** Display text per tile; null while that sensor has not reported. */
        @JvmField val values = arrayOfNulls<String>(READING_COUNT)

        /** Whether the tile's reading is past its alert threshold. */
        @JvmField val breached = BooleanArray(READING_COUNT)
    }

    private val tiles: LiveData<Tiles> = Transformations.map(readings.readings()) { toTiles(it) }

    fun tiles(): LiveData<Tiles> = tiles

    private fun toTiles(r: Readings): Tiles {
        val t = Tiles()
        val f = formatter
        if (r.isValid(IDX_TEMPERATURE)) {
            val v = r.get(IDX_TEMPERATURE)
            t.values[IDX_TEMPERATURE] = f.formatTemp(v)
            t.breached[IDX_TEMPERATURE] = thresholds.tempBreached(v)
        }
        if (r.isValid(IDX_HUMIDITY)) {
            val v = r.get(IDX_HUMIDITY)
            t.values[IDX_HUMIDITY] = f.formatHumidity(v)
            t.breached[IDX_HUMIDITY] = thresholds.humidityBreached(v)
        }
        if (r.isValid(IDX_PRESSURE)) {
            t.values[IDX_PRESSURE] = f.formatPressure(r.get(IDX_PRESSURE))
        }
        if (r.isValid(IDX_GAS)) {
            t.values[IDX_GAS] = f.formatGasIaq(r.get(IDX_GAS))
        }
        if (r.isValid(IDX_LIGHT)) {
            val v = r.get(IDX_LIGHT)
            t.values[IDX_LIGHT] = f.formatLux(v)
            t.breached[IDX_LIGHT] = thresholds.luxBreached(v)
        }
        return t
    }

    /** Makes the [LiveViewModel] with the app's singletons. */
    class Factory
    @Inject
    constructor(
        private val readings: LatestReadings,
        private val formatter: Formatter,
        private val thresholds: ThresholdConfig,
    ) : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            if (modelClass !== LiveViewModel::class.java) {
                throw IllegalArgumentException("not a LiveViewModel: " + modelClass.getName())
            }
            @Suppress("UNCHECKED_CAST")
            return LiveViewModel(readings, formatter, thresholds) as T
        }
    }
}
