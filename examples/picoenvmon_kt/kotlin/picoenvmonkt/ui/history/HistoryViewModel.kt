// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.history

import picodroid.lifecycle.LiveData
import picodroid.lifecycle.MutableLiveData
import picodroid.lifecycle.ViewModel
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.RING_CAPACITY
import picoenvmonkt.service.SensorLoggerService

/**
 * What the History screen shows: one snapshot of the logging service's temperature ring, taken when
 * the screen binds the service. The Activity renders it; the info dialog reads a sample back from
 * the same snapshot, so the row and the dialog always agree. A public no-argument constructor, so
 * the default factory makes it.
 */
class HistoryViewModel : ViewModel() {

    /** The ring, oldest sample first. */
    class History {
        @JvmField val samples = FloatArray(RING_CAPACITY)

        /** Epoch seconds per sample; 0 = taken before the wall clock was set. */
        @JvmField val timestamps = IntArray(RING_CAPACITY)

        @JvmField var count = 0
    }

    private val history = MutableLiveData<History>()

    /** Null until [load] has run. */
    fun history(): LiveData<History> = history

    /** Take a fresh snapshot of `service`'s temperature ring and publish it. */
    fun load(service: SensorLoggerService) {
        val h = History()
        h.count = service.snapshot(IDX_TEMPERATURE, h.samples, h.timestamps)
        history.setValue(h)
    }
}
