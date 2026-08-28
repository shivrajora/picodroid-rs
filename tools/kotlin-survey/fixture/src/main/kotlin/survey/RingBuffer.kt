// SPDX-License-Identifier: GPL-3.0-only
package survey

/**
 * SensorRingBuffer shape: FloatArray/IntArray storage and the ArraysKt surface a ring buffer
 * touches.
 */
class RingBuffer(capacity: Int) {
    private val data = FloatArray(capacity)
    private val ts = IntArray(capacity)
    private var head = 0
    private var size = 0

    fun add(sample: Float, epochSec: Int = 0) {
        data[head] = sample
        ts[head] = epochSec
        head = (head + 1) % data.size
        if (size < data.size) size++
    }

    fun snapshot(out: FloatArray): Int {
        val n = minOf(size, out.size)
        var idx = (head - n + data.size) % data.size
        for (i in 0 until n) {
            out[i] = data[idx]
            idx = (idx + 1) % data.size
        }
        return n
    }

    fun max(): Float? = data.maxOrNull()

    fun average(): Float = data.average().toFloat()

    fun sum(): Float = data.sum()

    fun copy(): FloatArray = data.copyOf()

    fun clear() {
        data.fill(0f)
        ts.fill(0)
        head = 0
        size = 0
    }

    fun spread(): Float {
        var lo = Float.MAX_VALUE
        var hi = -Float.MAX_VALUE
        for (v in data) {
            if (v < lo) lo = v
            if (v > hi) hi = v
        }
        return hi - lo
    }

    fun last(): Float = data[data.lastIndex]

    fun newest(): Int = ts[(head - 1 + ts.size) % ts.size]
}
