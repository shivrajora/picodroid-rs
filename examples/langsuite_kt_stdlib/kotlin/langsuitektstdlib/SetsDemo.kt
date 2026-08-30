// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * `Set` idioms over `SetsKt` and the `HashSet` builtin (hash-ordered: nothing here asserts order).
 */
object SetsDemo {
    private const val TAG = "SetsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    fun run() {
        Log.i(TAG, "=== Sets Tests ===")

        val s = setOf(3, 1, 4, 1, 5)
        check("setOf dedups", s.size == 4)
        check("contains / in", 4 in s && 9 !in s && s.contains(1))
        check("isEmpty", !s.isEmpty() && emptySet<Int>().isEmpty() && setOf<Int>().isEmpty())
        var sum = 0
        for (x in s) sum += x
        check("for in set", sum == 13)
        check("sum / max / min", s.sum() == 13 && s.max() == 5 && s.minOrNull() == 1)
        check(
            "map / filter / count",
            s.map { it * 2 }.sum() == 26 &&
                s.filter { it > 2 }.size == 3 &&
                s.count { it % 2 == 1 } == 3,
        )
        check("toList sorted", s.toList().sorted().joinToString() == "1, 3, 4, 5")
        check("sorted directly", s.sorted().first() == 1)
        check("joinToString", s.sorted().joinToString("+") == "1+3+4+5")
        check(
            "toSet / distinct",
            listOf(1, 1, 2).toSet().size == 2 && listOf(1, 1, 2).distinct().size == 2,
        )
        val ms = mutableSetOf<String>()
        check("mutableSetOf add", ms.add("a") && !ms.add("a") && ms.add("b") && ms.size == 2)
        check("remove", ms.remove("a") && !ms.remove("zz") && ms.size == 1)
        ms += "c"
        ms += listOf("d", "e")
        check("plusAssign", ms.size == 4 && "e" in ms)
        ms -= "c"
        check("minusAssign", ms.size == 3 && "c" !in ms)
        val a = setOf(1, 2, 3)
        val b = setOf(2, 3, 4)
        check("union", (a union b).size == 4 && (a + b).size == 4)
        check("intersect", (a intersect b).sorted().joinToString() == "2, 3")
        check(
            "subtract / minus",
            (a subtract b).sorted().joinToString() == "1" && (a - b).size == 1 && (a - 2).size == 2,
        )
        check("plus element", (a + 9).size == 4 && a.size == 3)
        check("any / all / none", a.any { it > 2 } && a.all { it > 0 } && a.none { it > 3 })
        check("setOf single", setOf("x").size == 1 && "x" in setOf("x"))
        check("hashSetOf / linkedSetOf", hashSetOf(1, 2, 2).size == 2 && linkedSetOf("a").size == 1)
        check("mutableSetOf(elems)", mutableSetOf(1, 2, 3).size == 3)
        check("toMutableSet", a.toMutableSet().also { it.add(7) }.size == 4 && a.size == 3)
        val strs = setOf("kotlin", "pico")
        check("string set", "pico" in strs && strs.map { it.length }.sum() == 10)
        check(
            "first / firstOrNull",
            setOf(42).first() == 42 && emptySet<Int>().firstOrNull() == null,
        )
        check("isNotEmpty / template size", strs.isNotEmpty() && "${strs.size}" == "2")
        val fromChars = "hello".toSet()
        check("String.toSet", fromChars.size == 4 && 'l' in fromChars)
        val set2 = listOf(5, 5, 6).toHashSet()
        check("toHashSet", set2.size == 2)
        check("partition of set", a.partition { it > 1 }.first.size == 2)
        check(
            "all in set via containsAll-free loop",
            listOf(1, 2).all { it in a } && !listOf(1, 9).all { it in a },
        )
        check("set of pairs / data", setOf(1 to 2).size == 1)
        check("clear", ms.also { it.clear() }.isEmpty())
        check(
            "set from range / dedup arrays",
            (1..3).toSet().size == 3 && intArrayOf(2, 2).toSet().size == 1,
        )
        check("map keys are a set", mapOf(1 to 1, 2 to 2).keys.filter { it > 1 }.size == 1)
        check("sumOf / maxOf on set", a.sumOf { it * 2 } == 12 && a.maxOf { it } == 3)
        check("joinToString transform on set", a.sorted().joinToString("") { "${it * 2}" } == "246")

        // S9: emptySet() must not be a shared mutable singleton.
        @Suppress("UNCHECKED_CAST")
        (emptySet<Int>() as MutableSet<Int>).add(7)
        check("emptySet not shared", emptySet<Int>().isEmpty())

        Check.done(TAG)
    }
}
