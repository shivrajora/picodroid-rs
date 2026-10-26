// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.data

/**
 * One second's smoothed values, as [LatestReadings.readings] publishes them: immutable, so an
 * observer may keep it. Indexed by `IDX_TEMPERATURE` and its siblings.
 */
class Readings internal constructor(private val values: FloatArray, private val validMask: Int) {
    /** Whether `idx` has received at least one sample. */
    fun isValid(idx: Int): Boolean = (validMask and (1 shl idx)) != 0

    fun get(idx: Int): Float = values[idx]
}
