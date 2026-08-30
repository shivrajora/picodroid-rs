// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.data

/**
 * Fixed-capacity circular buffer of float samples plus a parallel epoch-second timestamp per sample
 * (0 = wall clock not yet NTP-synced when captured). Overwrites oldest on full. Primitive arrays
 * only, allocated once — zero per-sample churn. Two explicit `snapshot` overloads rather than a
 * default parameter: a default emits a synthetic `$default` bridge method.
 */
class SensorRingBuffer(capacity: Int) {
    private val data = FloatArray(capacity)
    private val ts = IntArray(capacity) // epoch seconds; 0 = unknown (int is fine until 2038)
    private var head = 0 // next write index
    private var size = 0 // number of valid samples (≤ capacity)

    fun add(sample: Float, epochSec: Int) {
        data[head] = sample
        ts[head] = epochSec
        head = (head + 1) % data.size
        if (size < data.size) {
            size++
        }
    }

    /** Copy oldest-first samples into `out`. Returns the number of samples written. */
    fun snapshot(out: FloatArray): Int {
        val n = size
        val start = (head - size + data.size) % data.size
        var i = 0
        while (i < n && i < out.size) {
            out[i] = data[(start + i) % data.size]
            i++
        }
        return n
    }

    /** As [snapshot], also copying each sample's epoch-second timestamp. */
    fun snapshot(out: FloatArray, tsOut: IntArray): Int {
        val n = snapshot(out)
        val start = (head - size + data.size) % data.size
        var i = 0
        while (i < n && i < tsOut.size) {
            tsOut[i] = ts[(start + i) % data.size]
            i++
        }
        return n
    }
}
