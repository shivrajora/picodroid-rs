// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.util

import javax.inject.Inject
import javax.inject.Singleton

/**
 * Sensor-value → display-string formatter. The °C↔°F flag is global to the app. The Java app's
 * static helpers (`centi`, `iaqFromGas`) are instance methods here: every caller already holds the
 * injected singleton, and that keeps the class count where Java's is.
 */
@Singleton
class Formatter @Inject constructor() {
    /** `isFahrenheit()` / `setFahrenheit(boolean)` on the JVM, as in the Java app. */
    var isFahrenheit = false

    fun formatTemp(celsius: Float): String =
        centiToString(tempCenti(celsius)) + (if (isFahrenheit) "F" else "C")

    /**
     * Temperature in centi-units of the active scale — the allocation-free twin of [formatTemp] for
     * byte-path renderers (the HTTP dashboard).
     */
    fun tempCenti(celsius: Float): Int {
        if (isFahrenheit) {
            val milliF = (celsius * 1.8f * 1000).toInt() + 32_000
            return milliF / 10
        }
        return (celsius * 100).toInt()
    }

    /** Value → fixed-point centi units (12.34 → 1234), for byte-path renderers. */
    fun centi(v: Float): Int = (v * 100).toInt()

    fun formatHumidity(pct: Float): String = centiToString((pct * 100).toInt()) + " %"

    /** Pressure in hPa. */
    fun formatPressure(hpa: Float): String = centiToString((hpa * 100).toInt()) + " hPa"

    fun formatLux(lux: Float): String = "${lux.toInt()} lx"

    /** Quick IAQ index from a gas-resistance reading. Higher gas resistance → cleaner air. */
    fun formatGasIaq(gasOhm: Float): String {
        if (gasOhm <= 0f) {
            // ASCII placeholder — the bundled font has no em-dash (U+2014) glyph.
            return "--"
        }
        return "${iaqFromGas(gasOhm)} IAQ"
    }

    /**
     * 0..500 IAQ index, log-scaled around a 50 kΩ "average indoor" reference. Not a calibrated
     * index; useful as a comparative trend indicator only.
     */
    fun iaqFromGas(gasOhm: Float): Int {
        if (gasOhm <= 1f) {
            return 500
        }
        val ref = 50_000f
        val ratio = gasOhm / ref
        if (ratio <= 0.001f) {
            return 500
        }
        if (ratio >= 4f) {
            return 0
        }
        var iaq = 250 - (log2(ratio) * 60f).toInt()
        if (iaq < 0) {
            iaq = 0
        }
        if (iaq > 500) {
            iaq = 500
        }
        return iaq
    }

    private fun log2(x: Float): Float {
        var n = 0
        var v = x
        while (v > 1f) {
            v *= 0.5f
            n++
        }
        while (v < 0.5f) {
            v *= 2f
            n--
        }
        return n.toFloat() + (v - 0.5f) * 2f - 0.25f
    }

    /** "1234" → "12.34" — fixed two-decimal formatter without floats. */
    private fun centiToString(centi: Int): String {
        val neg = centi < 0
        val abs = if (neg) -centi else centi
        val whole = abs / 100
        val frac = abs % 100
        val fracStr = if (frac < 10) "0$frac" else frac.toString()
        val body = "$whole.$fracStr"
        return if (neg) "-$body" else body
    }

    /**
     * ASCII bytes of a constant string — `String.getBytes()`, the one encoding call pico-jvm
     * serves. Kotlin's `toByteArray()` inlines to the `Charset` overload (plus
     * `kotlin.text.Charsets`) and would fail the shim contract, hence the platform-class cast.
     */
    @Suppress("PLATFORM_CLASS_MAPPED_TO_KOTLIN")
    fun ascii(s: String): ByteArray = (s as java.lang.String).bytes
}
