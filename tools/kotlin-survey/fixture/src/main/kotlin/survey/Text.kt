// SPDX-License-Identifier: GPL-3.0-only
package survey

import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sqrt
import picodroid.util.Log

/** Extension functions (Formatter/TimeFormat shape). */
fun Float.fmt1(): String = String.format("%.1f", this)

fun String?.orDash(): String = this ?: "-"

/** Constructor default arguments → `DefaultConstructorMarker` in the synthetic ctor descriptor. */
class Cfg(val a: Int = 1, val b: String = "x")

/**
 * Strings, chars, templates, kotlin.math, default/named args, varargs, scope functions,
 * require/check/error/TODO.
 */
object TextDemo {
    private const val TAG = "Text"

    fun fmt(v: Float, digits: Int = 1, unit: String = ""): String =
        String.format("%.${digits}f%s", v, unit)

    fun logAll(vararg parts: String) {
        for (p in parts) Log.d(TAG, p)
    }

    fun strings(raw: String, maybe: Int?, reading: Reading, value: Float): String {
        val t = raw.trim()
        val parts = t.split(",")
        val padded = t.padStart(5, '0')
        val n = t.toIntOrNull() ?: -1
        val before = t.substringBefore(':')
        val has = t.contains("x", ignoreCase = true)
        val starts = t.startsWith("ab")
        val blank = t.isBlank()
        val c = t.firstOrNull() ?: ' '
        val digit = c.isDigit()
        val d = c - '0'
        val code = c.code
        val lower = c in 'a'..'z'
        val up = c.uppercaseChar()
        val tpl = "$maybe $reading $value ${reading.value} ${maybe ?: 0}"
        val m = abs(value) + sqrt(value) + max(1f, value) + value.pow(2f) + floor(value)
        val r = value.roundToInt()
        val named = fmt(value, unit = "C")
        val cfg = Cfg()
        val cfg2 = Cfg(b = "y")
        val arr = arrayOf("a", "b")
        logAll(*arr)
        logAll("one", "two")
        val applied =
            StringBuilder().apply { append(t) }.let { it.length }.also { Log.d(TAG, "len $it") }
        val ran = t.run { length + 1 }
        val w = with(t) { length }
        val tk = t.takeIf { it.isNotEmpty() }
        require(value >= 0f) { "negative $value" }
        check(n >= -1) { "n out of range" }
        val ns: String? = null
        val plus = ns + "x"
        val dash = ns.orDash()
        return "parts=${parts.size} padded=$padded n=$n before=$before has=$has starts=$starts blank=$blank" +
            " digit=$digit d=$d code=$code lower=$lower up=$up tpl=$tpl m=$m r=$r named=$named" +
            " cfg=${cfg.a}${cfg.b} cfg2=${cfg2.a}${cfg2.b} applied=$applied ran=$ran w=$w tk=$tk plus=$plus dash=$dash"
    }

    fun later(): Nothing = TODO("later")

    fun fail(): Nothing = error("boom")
}
