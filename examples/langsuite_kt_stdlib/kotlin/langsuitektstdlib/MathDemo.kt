// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import kotlin.math.E
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.absoluteValue
import kotlin.math.atan2
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.ln
import kotlin.math.log
import kotlin.math.log10
import kotlin.math.log2
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.roundToLong
import kotlin.math.sign
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.math.tan
import kotlin.math.truncate
import picodroid.util.Log

/**
 * `kotlin.math` (inline to `java.lang.Math` or served by `MathKt`), numeric conversions, bit ops.
 */
object MathDemo {
    private const val TAG = "MathKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Int): Int = v

    private fun near(a: Double, b: Double) = abs(a - b) < 1e-9

    private fun nearf(a: Float, b: Float) = abs(a - b) < 1e-5f

    /**
     * `isNaN()` inlines to the `Float.isNaN(F)` static, which pico-jvm does not serve; IEEE
     * self-inequality does.
     */
    private fun nan(f: Float) = f != f

    private fun nan(d: Double) = d != d

    fun run() {
        Log.i(TAG, "=== Math Tests ===")

        check(
            "abs",
            abs(-5) == 5 &&
                abs(-2.5f) == 2.5f &&
                abs(-1.5) == 1.5 &&
                (-3).absoluteValue == 3 &&
                abs(-4L) == 4L,
        )
        check(
            "sqrt / pow",
            sqrt(16.0) == 4.0 && 2.0.pow(10) == 1024.0 && 2.0.pow(0.5) > 1.41 && 3f.pow(2) == 9f,
        )
        check(
            "floor / ceil / truncate",
            floor(2.7) == 2.0 &&
                floor(-2.1) == -3.0 &&
                ceil(2.1) == 3.0 &&
                truncate(-2.7) == -2.0 &&
                truncate(2.7) == 2.0,
        )
        check(
            "roundToInt float",
            2.5f.roundToInt() == 3 &&
                2.4f.roundToInt() == 2 &&
                (-2.5f).roundToInt() == -2 &&
                3.7f.roundToInt() == 4,
        )
        check(
            "roundToInt double",
            2.5.roundToInt() == 3 && (-3.5).roundToInt() == -3 && 1.49.roundToInt() == 1,
        )
        check("roundToLong", 2.5.roundToLong() == 3L && 1e10.roundToLong() == 10000000000L)
        check(
            "max / min",
            max(3, 7) == 7 &&
                min(3, 7) == 3 &&
                max(1.5f, 0.5f) == 1.5f &&
                min(1.5, 2.5) == 1.5 &&
                max(2L, 1L) == 2L,
        )
        check(
            "maxOf / minOf",
            maxOf(1, 2) == 2 &&
                maxOf(1, 5, 3) == 5 &&
                minOf(4, 2, 9) == 2 &&
                maxOf(1.5f, 2.5f) == 2.5f &&
                minOf("b", "a") == "a",
        )
        check(
            "sin / cos / tan",
            near(sin(0.0), 0.0) &&
                near(cos(0.0), 1.0) &&
                near(sin(PI / 2), 1.0) &&
                near(tan(0.0), 0.0),
        )
        check("atan2 / PI / E", near(atan2(1.0, 1.0), PI / 4) && PI > 3.14 && PI < 3.15 && E > 2.71)
        check(
            "ln / log10 / log2 / log / exp",
            near(ln(E), 1.0) &&
                near(log10(1000.0), 3.0) &&
                near(log2(8.0), 3.0) &&
                near(log(8.0, 2.0), 3.0) &&
                near(exp(0.0), 1.0),
        )
        check("sqrt float", nearf(sqrt(2f), 1.41421f))
        check(
            "Int division / rem",
            7 / 2 == 3 && -7 / 2 == -3 && 7 % 3 == 1 && (-7) % 3 == -1 && 7.rem(3) == 1,
        )
        check("mod (Euclidean)", (-7).mod(3) == 2 && 7.mod(-3) == -2)
        check("float ops", 0.1f + 0.2f > 0.3f - 0.01f && 1.0f / 3.0f < 0.34f && (10f / 4f) == 2.5f)
        check("double ops", near(0.1 + 0.2, 0.3))
        check(
            "conversions",
            3.99.toInt() == 3 &&
                (-3.99).toInt() == -3 &&
                300.toByte() == 44.toByte() &&
                65.toChar() == 'A' &&
                7.toFloat() == 7f &&
                2.5f.toDouble() == 2.5 &&
                7L.toInt() == 7 &&
                1e3.toFloat() == 1000f,
        )
        check(
            "toString of numbers",
            1.5f.toString() == "1.5" &&
                42.toString() == "42" &&
                2.0.toString() == "2.0" &&
                (-7L).toString() == "-7",
        )
        val zero = opaque(0)
        val nanf = zero.toFloat() / zero.toFloat()
        val inf = 1.0 / zero.toDouble()
        check("NaN / infinity", nan(nanf) && !nan(1.0) && inf > 1e308 && -inf < -1e308)
        check(
            "Int limits / overflow wrap",
            Int.MAX_VALUE == 2147483647 &&
                opaque(Int.MAX_VALUE) + 1 == Int.MIN_VALUE &&
                Long.MAX_VALUE > 0 &&
                Byte.MAX_VALUE == 127.toByte(),
        )
        check(
            "bit ops",
            (5 and 3) == 1 &&
                (5 or 3) == 7 &&
                (5 xor 3) == 6 &&
                (1 shl 4) == 16 &&
                (-16 shr 2) == -4 &&
                (-16 ushr 28) == 15 &&
                5.inv() == -6,
        )
        check(
            "popcount loop",
            run {
                var b = 0
                var v = 0b1011
                while (v != 0) {
                    b += v and 1
                    v = v ushr 1
                }
                b == 3
            },
        )
        check("hex / bin literals / underscores", 0xFF == 255 && 0b101 == 5 && 1_000_000 == 1000000)
        check(
            "compareTo on numbers",
            3.compareTo(5) < 0 &&
                2.5f.compareTo(2.5f) == 0 &&
                3L.compareTo(2L) > 0 &&
                1.0.compareTo(2.0) < 0,
        )
        check(
            "boxed math",
            listOf(1.5, 2.5).sum() == 4.0 &&
                listOf(3, 4).maxOf { it * it } == 16 &&
                listOf(1, 2, 3).sumOf { it.toDouble() } == 6.0,
        )
        check("Int.sign", 5.sign == 1 && (-9).sign == -1 && 0.sign == 0)
        check("hypot-ish", near(sqrt(3.0 * 3.0 + 4.0 * 4.0), 5.0))
        check("coerce in math", 1.7.coerceIn(0.0, 1.0) == 1.0)
        check(
            "Long math",
            1L shl 40 == 1099511627776L &&
                1099511627776L / 1024 == 1073741824L &&
                (Long.MAX_VALUE / 2) > Int.MAX_VALUE,
        )
        check("Float precision", 16777216f + 1f == 16777216f)
        check("Double.toString precision", (1.0 / 3).toString().startsWith("0.3333"))
        check("Int.toString radix-free", 255.toString() == "255" && (-1).toString() == "-1")
        check("Char arithmetic to Int", ('z' - 'a') == 25 && ('a' + 25) == 'z')
        check("Long literal overflow-free", 4_000_000_000L > Int.MAX_VALUE)
        check(
            "Short / Byte arithmetic",
            (1.toShort() + 2.toShort()) == 3 && (100.toByte() + 100.toByte()) == 200,
        )
        check(
            "Float to Int truncation",
            2.99f.toInt() == 2 && (-2.99f).toInt() == -2 && 2.5f.roundToInt() == 3,
        )
        check(
            "integer pow via loop",
            run {
                var p = 1
                repeat(10) { p *= 2 }
                p == 1024
            },
        )
        check(
            "Double compare NaN",
            !(nanf.toDouble() > 0.0) && !(nanf.toDouble() < 0.0) && nanf.toDouble() != 0.0,
        )
        check("unsigned-like via toLong", (opaque(-1).toLong() and 0xFFFFFFFFL) == 4294967295L)

        Check.done(TAG)
    }
}
