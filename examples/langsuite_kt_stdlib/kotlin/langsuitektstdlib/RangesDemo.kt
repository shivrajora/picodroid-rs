// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * Ranges and progressions: the `for`-loop forms kotlinc intrinsifies (`..`, `until`, `downTo`,
 * `step`, `indices`, `reversed()`), the value forms that need `IntRange` / `IntProgression` /
 * `RangesKt` from the shim, and `coerceIn` / `coerceAtLeast` / `coerceAtMost`.
 */
object RangesDemo {
    private const val TAG = "RangesKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    fun run() {
        Log.i(TAG, "=== Ranges Tests ===")

        var s = 0
        for (i in 1..5) s += i
        check("for in 1..5", s == 15)
        s = 0
        for (i in 0 until 4) s += i
        check("until loop", s == 6)
        s = 0
        for (i in 5 downTo 1) s = s * 10 + i
        check("downTo loop", s == 54321)
        s = 0
        for (i in 0..10 step 3) s += i
        check("step loop", s == 18)
        s = 0
        for (i in 10 downTo 0 step 4) s += i
        check("downTo step loop", s == 18)
        s = 0
        for (i in (1..4).reversed()) s = s * 10 + i
        check("reversed loop", s == 4321)

        val r = 1..5
        check("first / last / step", r.first == 1 && r.last == 5 && r.step == 1)
        check("in range value", 3 in r && 6 !in r && r.contains(5))
        check("count / sum", r.count() == 5 && r.sum() == 15)
        check("toList", r.toList().joinToString() == "1, 2, 3, 4, 5")
        check("map / filter", r.map { it * it }.last() == 25 && r.filter { it % 2 == 0 }.size == 2)
        check("isEmpty", !r.isEmpty() && (5..1).isEmpty() && (1 until 1).isEmpty())
        check(
            "toString",
            r.toString() == "1..5" &&
                (0 until 3).toString() == "0..2" &&
                (5 downTo 1).toString() == "5 downTo 1 step 1",
        )
        check("equals", r == 1..5 && r != 1..6)
        val u = 0 until 3
        check("until value", u.last == 2 && u.toList().size == 3)
        val d = 5 downTo 1
        check(
            "downTo value",
            d.first == 5 &&
                d.last == 1 &&
                d.step == -1 &&
                d.toList().joinToString() == "5, 4, 3, 2, 1",
        )
        val st = 1..10 step 4
        check("step value", st.toList().joinToString() == "1, 5, 9" && st.last == 9)
        check("reversed value", (1..3).reversed().toList().joinToString() == "3, 2, 1")
        check("joinToString on range", (1..3).joinToString("-") == "1-2-3")
        check("max / min / average", r.max() == 5 && r.minOrNull() == 1 && r.average() == 3.0)
        check("any / all / none", r.any { it == 4 } && r.all { it < 6 } && r.none { it > 5 })
        check("first {} / indexOf", r.first { it > 2 } == 3 && r.indexOf(3) == 2)
        check("fold", r.fold(1) { a, v -> a * v } == 120)
        check(
            "coerceIn",
            15.coerceIn(0, 10) == 10 && (-3).coerceIn(0, 10) == 0 && 5.coerceIn(0, 10) == 5,
        )
        check("coerceAtLeast / coerceAtMost", 3.coerceAtLeast(5) == 5 && 3.coerceAtMost(2) == 2)
        check(
            "coerceAtLeast / coerceAtMost widths",
            7L.coerceAtLeast(9L) == 9L &&
                7L.coerceAtMost(5L) == 5L &&
                2f.coerceAtLeast(3f) == 3f &&
                2f.coerceAtMost(1f) == 1f &&
                1.5.coerceAtLeast(2.0) == 2.0 &&
                1.5.coerceAtMost(1.0) == 1.0,
        )
        check(
            "coerceIn float / double / long",
            1.5f.coerceIn(0f, 1f) == 1f &&
                (-0.5).coerceIn(0.0, 1.0) == 0.0 &&
                7L.coerceIn(1L, 5L) == 5L,
        )
        check("coerceIn range", 12.coerceIn(1..10) == 10 && 0.coerceIn(1..10) == 1)
        check(
            "Char range loop",
            run {
                var c = ""
                for (ch in 'a'..'e') c += ch
                c == "abcde"
            },
        )
        check("in char range", 'c' in 'a'..'z' && '1' !in 'a'..'z')
        check("in int range literal", 5 in 1..10 && 11 !in 1..10)
        check(
            "until step + reversed",
            (0 until 10 step 3).reversed().toList().joinToString() == "9, 6, 3, 0",
        )
        var reps = 0
        repeat(3) { reps += it }
        check("repeat", reps == 3)
        check("indices of list", listOf(1, 2, 3).indices == 0..2)
        var evens = 0
        for (i in intArrayOf(1, 2, 3, 4).indices step 2) evens += i
        check("indices step", evens == 2)
        val big = 1..1000
        check("large range count", big.count() == 1000 && big.sum() == 500500)
        check("downTo empty", (1 downTo 5).isEmpty() && (1 downTo 5).toList().isEmpty())
        check("progression last clamps", (1..10 step 3).last == 10 && (1..11 step 3).last == 10)
        check("range hashCode stable", (1..5).hashCode() == (1..5).hashCode())
        check(
            "range in when",
            when (7) {
                in 1..5 -> "low"
                in 6..10 -> "mid"
                else -> "hi"
            } == "mid",
        )
        check(
            "range in when value",
            run {
                val v = 4
                when (v) {
                    in r -> true
                    else -> false
                }
            },
        )
        check("random-free pick by index", r.toList()[r.count() - 1] == 5)
        check("until with negative", (-2 until 1).toList().joinToString() == "-2, -1, 0")
        check("step on downTo value", (10 downTo 1 step 5).toList().joinToString() == "10, 5")
        check("sumOf over range", (1..4).sumOf { it * it } == 30)
        check(
            "forEach over range",
            run {
                var t = 0
                (1..3).forEach { t += it }
                t == 6
            },
        )
        check("mapIndexed over range", (5..7).mapIndexed { i, v -> i * v }.sum() == 6 + 14)
        check(
            "Long range loop",
            run {
                var t = 0L
                for (i in 1L..3L) t += i
                t == 6L
            },
        )

        Check.done(TAG)
    }
}
