// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * `IntArray` / `FloatArray` / `Array<T>` idioms over `ArraysKt` and the `java.util.Arrays`
 * builtins.
 */
object ArraysDemo {
    private const val TAG = "ArraysKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    fun run() {
        Log.i(TAG, "=== Arrays Tests ===")

        val ints = intArrayOf(4, 2, 9, 1)
        check("size / index", ints.size == 4 && ints[2] == 9)
        var s = 0
        for (v in ints) s += v
        check("for in", s == 16)
        var idx = 0
        for (i in ints.indices) idx += i
        check("indices", idx == 6)
        check("sum / average", ints.sum() == 16 && ints.average() == 4.0)
        check(
            "max / min",
            ints.max() == 9 &&
                ints.min() == 1 &&
                ints.maxOrNull() == 9 &&
                intArrayOf().minOrNull() == null,
        )
        check("contains / in", 9 in ints && 7 !in ints)
        check(
            "indexOf / lastIndex",
            ints.indexOf(9) == 2 && ints.lastIndex == 3 && ints.indexOf(7) == -1,
        )
        check("first / last", ints.first() == 4 && ints.last() == 1)
        check("toList", ints.toList().size == 4 && ints.toList()[1] == 2)
        check("sorted", ints.sorted().joinToString() == "1, 2, 4, 9")
        check("sortedArray", ints.sortedArray().last() == 9 && ints[0] == 4)
        val copy = ints.copyOf()
        copy.sort()
        check("copyOf / sort in place", copy[0] == 1 && ints[0] == 4)
        check("copyOfRange", ints.copyOfRange(1, 3).joinToString() == "2, 9")
        check(
            "reversed / reversedArray",
            ints.reversed().first() == 1 && ints.reversedArray()[0] == 1,
        )
        copy.reverse()
        check("reverse in place", copy[0] == 9)
        check(
            "joinToString",
            ints.joinToString("-") == "4-2-9-1" &&
                ints.joinToString(prefix = "<", postfix = ">") == "<4, 2, 9, 1>",
        )
        check("map / filter", ints.map { it * 2 }.sum() == 32 && ints.filter { it > 3 }.size == 2)
        check(
            "any / all / count",
            ints.any { it == 9 } && ints.all { it > 0 } && ints.count { it % 2 == 0 } == 2,
        )
        var fe = 0
        ints.forEach { fe += it }
        ints.forEachIndexed { i, v -> fe += i * v }
        check("forEach / forEachIndexed", fe == 16 + 2 + 18 + 3)
        val filled = IntArray(3)
        filled.fill(7)
        check("IntArray(n) / fill", filled.joinToString() == "7, 7, 7")
        filled.fill(1, 1, 3)
        check("fill range", filled.joinToString() == "7, 1, 1")
        val sq = IntArray(4) { it * it }
        check("IntArray init", sq.joinToString() == "0, 1, 4, 9")
        check(
            "contentToString",
            ints.contentToString() == "[4, 2, 9, 1]" && sq.contentToString() == "[0, 1, 4, 9]",
        )
        val floats = floatArrayOf(1.5f, 2.5f, 0.5f)
        check("FloatArray sum / average", floats.sum() == 4.5f && floats.average() == 1.5)
        check("FloatArray max / min", floats.max() == 2.5f && floats.minOrNull() == 0.5f)
        check("FloatArray sorted", floats.sorted()[0] == 0.5f)
        check(
            "FloatArray first / last / lastIndex",
            floats.first() == 1.5f && floats.last() == 0.5f && floats.lastIndex == 2,
        )
        check(
            "FloatArray maxOrNull / toList",
            floats.maxOrNull() == 2.5f &&
                floatArrayOf().maxOrNull() == null &&
                floats.toList()[2] == 0.5f,
        )
        val fa = FloatArray(2)
        fa.fill(3f)
        check("FloatArray fill", fa[1] == 3f)
        check(
            "toFloatArray / toIntArray",
            listOf(1f, 2f).toFloatArray().size == 2 && listOf(5, 6).toIntArray()[1] == 6,
        )
        val strs = arrayOf("b", "c", "a")
        check("Array<String> size / in", strs.size == 3 && "c" in strs && "z" !in strs)
        check(
            "Array sorted / joinToString",
            strs.sorted().joinToString("") == "abc" && strs.joinToString() == "b, c, a",
        )
        check("Array toList / asList", strs.toList()[2] == "a" && strs.asList().size == 3)
        check(
            "Array map / filter",
            strs.map { it.uppercase() }.joinToString("") == "BCA" &&
                strs.filter { it > "a" }.size == 2,
        )
        check(
            "Array indexOf / first / last",
            strs.indexOf("c") == 1 && strs.first() == "b" && strs.last() == "a",
        )
        val nullable = arrayOf("x", null, "y")
        check("Array<String?> filterNotNull", nullable.filterNotNull().size == 2)
        val typed = listOf("p", "q").toTypedArray()
        check("toTypedArray", typed.size == 2 && typed[1] == "q")
        val gen = Array(3) { "s$it" }
        check("Array(n) init", gen[2] == "s2" && gen.contentToString() == "[s0, s1, s2]")
        val objs = arrayOfNulls<String>(2)
        check("arrayOfNulls", objs.size == 2 && objs[0] == null)
        val la = longArrayOf(1L, 2L)
        val da = doubleArrayOf(1.0, 2.0)
        val ba = booleanArrayOf(true, false)
        val ca = charArrayOf('a', 'b')
        check(
            "other primitive arrays",
            la.sum() == 3L && da.sum() == 3.0 && ba.count { it } == 1 && ca[1] == 'b',
        )
        check("isEmpty / isNotEmpty", intArrayOf().isEmpty() && ints.isNotEmpty())
        check(
            "getOrNull / getOrElse",
            ints.getOrNull(9) == null && ints.getOrElse(9) { -1 } == -1 && ints.getOrNull(0) == 4,
        )
        check("2D array", Array(2) { IntArray(2) { it } }[1][1] == 1)
        check(
            "array of arrays contentDeep-free",
            arrayOf(intArrayOf(1), intArrayOf(2, 3))[1].size == 2,
        )
        check("take / drop on array", ints.take(2).sum() == 6 && ints.drop(3)[0] == 1)
        check("distinct on array", intArrayOf(1, 1, 2).distinct().size == 2)
        check("zip arrays", ints.zip(strs).size == 3)
        check(
            "withIndex on array",
            run {
                var t = 0
                for ((i, v) in ints.withIndex()) t += i * v
                t == 2 + 18 + 3
            },
        )
        check("sumOf on array", ints.sumOf { it * 10 } == 160)
        check(
            "maxBy / minByOrNull on array",
            strs.maxBy { it }.first() == 'c' && strs.minByOrNull { it }?.first() == 'a',
        )

        Check.done(TAG)
    }
}
