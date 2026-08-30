// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * `List` idioms over `CollectionsKt`: factories, accessors, the inline HOFs (which must leave no
 * `kotlin/…` call but their helpers), aggregation, slicing, joining, mutation.
 */
object CollectionsDemo {
    private const val TAG = "CollectionsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    /** Element-wise equality: builtin lists compare by identity on pico-jvm (documented). */
    private fun eq(a: List<*>, b: List<*>): Boolean {
        if (a.size != b.size) return false
        for (i in a.indices) if (a[i] != b[i]) return false
        return true
    }

    fun run() {
        Log.i(TAG, "=== Collections Tests ===")

        val nums = listOf(3, 1, 4, 1, 5)
        check("listOf size", nums.size == 5)
        check("get / index", nums[0] == 3 && nums.get(4) == 5)
        check("contains / in", 4 in nums && 9 !in nums)
        check("isEmpty / isNotEmpty", !nums.isEmpty() && nums.isNotEmpty())
        check("first / last", nums.first() == 3 && nums.last() == 5)
        check(
            "firstOrNull / lastOrNull",
            nums.firstOrNull() == 3 &&
                emptyList<Int>().firstOrNull() == null &&
                emptyList<Int>().lastOrNull() == null,
        )
        check("getOrNull", nums.getOrNull(1) == 1 && nums.getOrNull(7) == null)
        check("lastIndex", nums.lastIndex == 4)
        var idxSum = 0
        for (i in nums.indices) idxSum += i
        check("indices loop", idxSum == 10)
        check("indices value", nums.indices.last == 4)
        var wi = 0
        for ((i, v) in nums.withIndex()) wi += i * v
        check("withIndex loop", wi == 32)
        var fe = 0
        nums.forEachIndexed { i, v -> fe += i + v }
        check("forEachIndexed", fe == 24)
        check("map", eq(nums.map { it * 2 }, listOf(6, 2, 8, 2, 10)))
        check("filter", eq(nums.filter { it > 2 }, listOf(3, 4, 5)))
        check("filterNot", eq(nums.filterNot { it > 2 }, listOf(1, 1)))
        check(
            "any / all / none",
            nums.any { it == 4 } && nums.all { it > 0 } && nums.none { it > 5 },
        )
        check("count", nums.count() == 5 && nums.count { it == 1 } == 2)
        check("sum / sumOf", nums.sum() == 14 && nums.sumOf { it * it } == 52)
        check("average", nums.average() == 2.8)
        check("max / min", nums.max() == 5 && nums.min() == 1)
        check(
            "maxOrNull / minOrNull",
            nums.maxOrNull() == 5 && emptyList<Int>().maxOrNull() == null,
        )
        check(
            "fold / reduce",
            nums.fold(0) { acc, v -> acc + v } == 14 && nums.reduce { acc, v -> acc * v } == 60,
        )
        check("take / drop", eq(nums.take(2), listOf(3, 1)) && eq(nums.drop(3), listOf(1, 5)))
        check(
            "takeLast / dropLast",
            eq(nums.takeLast(2), listOf(1, 5)) && eq(nums.dropLast(4), listOf(3)),
        )
        check("reversed", eq(nums.reversed(), listOf(5, 1, 4, 1, 3)))
        check("distinct", eq(nums.distinct(), listOf(3, 1, 4, 5)))
        check(
            "zip",
            nums.zip(listOf("a", "b", "c")).size == 3 &&
                nums.zip(listOf("a", "b"))[1] == (1 to "b"),
        )
        check("joinToString default", nums.joinToString() == "3, 1, 4, 1, 5")
        check("joinToString sep", nums.joinToString("-") == "3-1-4-1-5")
        check("joinToString prefix/postfix", nums.joinToString(", ", "[", "]") == "[3, 1, 4, 1, 5]")
        check("joinToString transform", nums.joinToString(" ") { "<$it>" } == "<3> <1> <4> <1> <5>")
        check("joinToString limit", nums.joinToString(limit = 2) == "3, 1, ...")
        check("joinToString empty", emptyList<Int>().joinToString() == "")
        val words = listOf("kotlin", "on", "pico")
        check("joinToString strings", words.joinToString(separator = "/") == "kotlin/on/pico")

        val mut = mutableListOf(1, 2)
        mut.add(3)
        mut += 4
        mut += listOf(5, 6)
        check("mutableListOf add / plusAssign", mut.size == 6 && mut.last() == 6)
        mut[0] = 10
        check("set", mut[0] == 10)
        val removed = mut.removeAt(1)
        check("removeAt", removed == 2 && mut.size == 5 && mut[1] == 3)
        mut.reverse()
        check("reverse in place", mut.first() == 6 && mut.last() == 10)
        val copy = mut.toMutableList()
        copy.clear()
        check("toMutableList copy / clear", copy.isEmpty() && mut.size == 5)
        val plusList = nums + 6
        check("plus element", plusList.size == 6 && plusList.last() == 6 && nums.size == 5)
        check("plus list", (listOf(1) + listOf(2, 3)).size == 3)
        check("minus element (first occurrence)", eq(nums - 1, listOf(3, 4, 1, 5)))
        check(
            "listOfNotNull / filterNotNull",
            listOfNotNull(1, null, 2).size == 2 &&
                listOf(null, "a", null).filterNotNull().size == 1,
        )
        check(
            "flatMap / flatten",
            eq(listOf(1, 2).flatMap { listOf(it, it * 10) }, listOf(1, 10, 2, 20)) &&
                listOf(listOf(1), listOf(2, 3)).flatten().size == 3,
        )
        val groups = words.groupBy { it.length }
        check("groupBy", groups.size == 3 && groups[2]!!.size == 1 && groups[6]!![0] == "kotlin")
        val byLen = words.associateBy { it.length }
        check("associateBy", byLen[6] == "kotlin" && byLen.size == 3)
        val assoc = words.associate { it to it.length }
        check("associate", assoc["pico"] == 4)
        val (even, odd) = nums.partition { it % 2 == 0 }
        check("partition", eq(even, listOf(4)) && odd.size == 4)
        check("first {} / find", nums.first { it > 3 } == 4 && nums.find { it > 10 } == null)
        check(
            "indexOfFirst",
            nums.indexOfFirst { it == 1 } == 1 && nums.indexOfFirst { it == 7 } == -1,
        )
        check(
            "mapIndexed / mapNotNull",
            eq(nums.mapIndexed { i, v -> i + v }, listOf(3, 2, 6, 4, 9)) &&
                nums.mapNotNull { if (it > 3) it else null }.size == 2,
        )
        check(
            "sumOfFloat / averageOfDouble",
            listOf(1.5f, 2.5f).sum() == 4.0f && listOf(1.0, 2.0, 6.0).average() == 3.0,
        )
        check("sumOfLong", listOf(1L, 2L).sum() == 3L)
        check(
            "maxOrNull Float / max Double (return-type overloads)",
            listOf(1.5f, 0.5f).maxOrNull() == 1.5f &&
                listOf(1.0, 2.0).max() == 2.0 &&
                listOf(2.5f).min() == 2.5f,
        )
        val it2 = words.iterator()
        var n = 0
        while (it2.hasNext()) {
            it2.next()
            n++
        }
        check("explicit iterator", n == 3)
        check("emptyList", emptyList<String>().isEmpty() && emptyList<String>().size == 0)
        check("toList copy", eq(nums.toList(), nums))
        check(
            "first on empty throws",
            try {
                emptyList<Int>().first()
                false
            } catch (e: NoSuchElementException) {
                true
            },
        )
        check("contains string list", "pico" in words && !words.contains("java"))
        check("single", listOf(42).single() == 42)
        check("onEach", nums.onEach { n += it }.size == 5 && n == 17)
        check("chunk via windowed loop", nums.chunked(2).size == 3 && nums.chunked(2)[2].size == 1)

        // S9: emptyList() must not be a shared mutable singleton — one cast+add
        // used to poison every later emptyList() app-wide.
        @Suppress("UNCHECKED_CAST") (emptyList<Int>() as MutableList<Int>).add(99)
        check("emptyList not shared", emptyList<Int>().isEmpty() && listOf<Int>().isEmpty())

        Check.done(TAG)
    }
}
