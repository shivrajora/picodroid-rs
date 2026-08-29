// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * Sorting: `sorted*` (copying) and `sort*` (in place) on lists and arrays, `Comparable` user
 * classes, `compareBy` / `thenBy` / `naturalOrder` / `reverseOrder` from `ComparisonsKt`, SAM and
 * object `Comparator`s.
 */
object SortingDemo {
    private const val TAG = "SortingKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private class Person(val name: String, val age: Int) : Comparable<Person> {
        override fun compareTo(other: Person): Int = age - other.age

        override fun toString() = "$name:$age"
    }

    fun run() {
        Log.i(TAG, "=== Sorting Tests ===")

        val nums = listOf(5, 3, 9, 1)
        check(
            "sorted / sortedDescending",
            nums.sorted().joinToString() == "1, 3, 5, 9" &&
                nums.sortedDescending().first() == 9 &&
                nums.first() == 5,
        )
        val words = listOf("pear", "fig", "banana", "kiwi")
        check("sorted strings", words.sorted().joinToString() == "banana, fig, kiwi, pear")
        check(
            "sortedBy / sortedByDescending",
            words.sortedBy { it.length }.first() == "fig" &&
                words.sortedByDescending { it.length }.first() == "banana",
        )
        check(
            "sortedBy stable",
            words.sortedBy { it.length }.joinToString() == "fig, pear, kiwi, banana",
        )
        check(
            "sortedWith compareBy",
            words.sortedWith(compareBy { -it.length }).joinToString() == "banana, pear, kiwi, fig",
        )
        check(
            "compareBy two selectors",
            words.sortedWith(compareBy({ it.length }, { it })).joinToString() ==
                "fig, kiwi, pear, banana",
        )
        check(
            "compareBy thenBy",
            words.sortedWith(compareBy<String> { it.length }.thenBy { it }).joinToString() ==
                "fig, kiwi, pear, banana",
        )
        check(
            "thenByDescending",
            words
                .sortedWith(compareBy<String> { it.length }.thenByDescending { it })
                .joinToString() == "fig, pear, kiwi, banana",
        )
        check(
            "compareByDescending",
            words.sortedWith(compareByDescending { it.length }).first() == "banana",
        )
        check(
            "Comparator SAM lambda",
            words.sortedWith(Comparator { a, b -> b.length - a.length }).first() == "banana",
        )
        check(
            "Comparator object",
            words
                .sortedWith(
                    object : Comparator<String> {
                        override fun compare(a: String, b: String) = a.compareTo(b)
                    }
                )
                .first() == "banana",
        )
        val people = listOf(Person("cy", 40), Person("ann", 31), Person("bob", 25))
        check(
            "Comparable user class sorted",
            people.sorted().joinToString() == "bob:25, ann:31, cy:40",
        )
        check("sortedDescending user class", people.sortedDescending().first().name == "cy")
        check("sortedBy property", people.sortedBy { it.name }.first().name == "ann")
        check(
            "maxByOrNull / minByOrNull",
            people.maxByOrNull { it.age }?.name == "cy" &&
                people.minByOrNull { it.name }?.name == "ann",
        )
        check(
            "maxBy / minBy",
            people.maxBy { it.age }.name == "cy" && people.minBy { it.age }.name == "bob",
        )
        check("max / min Comparable", people.max().name == "cy" && people.min().name == "bob")
        check(
            "maxWith / minWithOrNull",
            people.maxWith(compareBy { it.name }).name == "cy" &&
                people.minWithOrNull(compareBy { it.name })?.name == "ann",
        )
        val mut = mutableListOf(4, 2, 8, 6)
        mut.sort()
        check("sort in place", mut.joinToString() == "2, 4, 6, 8")
        mut.sortDescending()
        check("sortDescending in place", mut.first() == 8)
        val mw = mutableListOf("bb", "a", "ccc")
        mw.sortBy { it.length }
        check("sortBy in place", mw.first() == "a")
        mw.sortByDescending { it.length }
        check("sortByDescending in place", mw.first() == "ccc")
        mw.sortWith(compareBy { it })
        check("sortWith in place", mw.joinToString() == "a, bb, ccc")
        check(
            "reversed after sort",
            nums.sorted().reversed().first() == 9 &&
                nums.sortedDescending().reversed().first() == 1,
        )
        check(
            "compareValues",
            compareValues(1, 2) < 0 &&
                compareValues("b", "a") > 0 &&
                compareValues(null, 1) < 0 &&
                compareValues(null, null) == 0,
        )
        check("compareValuesBy", compareValuesBy("ab", "c", { it.length }) > 0)
        check(
            "naturalOrder / reverseOrder",
            listOf(2, 1).sortedWith(naturalOrder()).first() == 1 &&
                listOf(1, 2).sortedWith(reverseOrder()).first() == 2,
        )
        val arr = intArrayOf(3, 1, 2)
        arr.sort()
        check("IntArray sort", arr.joinToString() == "1, 2, 3")
        arr.sortDescending()
        check("IntArray sortDescending", arr[0] == 3)
        val sarr = arrayOf("b", "a")
        sarr.sort()
        check("Array<String> sort", sarr[0] == "a")
        sarr.sortWith(compareByDescending { it })
        check("Array sortWith", sarr[0] == "b")
        check(
            "sorted on set / empty",
            setOf(3, 1).sorted().first() == 1 && emptyList<Int>().sorted().isEmpty(),
        )
        check("distinct then sorted", listOf(3, 3, 1).distinct().sorted().joinToString() == "1, 3")
        check("sortedBy last", listOf("b", "aa", "c").sortedBy { it.length }.last() == "aa")
        check(
            "zip sorted pairs",
            listOf(2, 1).zip(listOf("b", "a")).sortedBy { it.first }.first().second == "a",
        )
        check("indexOfFirst on sorted", nums.sorted().indexOfFirst { it > 4 } == 2)
        check(
            "sorted floats / longs",
            listOf(2.5f, 1.5f).sorted().first() == 1.5f &&
                listOf(3L, 1L).sortedDescending().first() == 3L,
        )
        check("sorted chars", listOf('b', 'a').sorted().first() == 'a')
        check(
            "sorted booleans-free: sortedBy boolean",
            listOf(3, 4, 5).sortedBy { it % 2 == 0 }.first() == 3,
        )
        check(
            "sortedWith comparator on data",
            listOf(1 to "b", 1 to "a")
                .sortedWith(compareBy({ it.first }, { it.second }))
                .first()
                .second == "a",
        )
        check(
            "sort stability with equal keys",
            listOf("bb", "aa", "c").sortedBy { it.length }.joinToString() == "c, bb, aa",
        )
        check(
            "comparator composition via lambda",
            words.sortedWith { a, b -> a.length.compareTo(b.length) }.first() == "fig",
        )
        check(
            "sortedBy nullable key",
            listOf("x", null, "yy").sortedBy { it?.length }.first() == null,
        )
        check(
            "Array sortedBy / sortedDescending",
            arrayOf("bb", "a").sortedBy { it.length }.first() == "a" &&
                arrayOf(1, 3, 2).sortedDescending().first() == 3,
        )
        check("Array<Int> sort (boxed Comparable)", arrayOf(3, 1, 2).also { it.sort() }[0] == 1)

        Check.done(TAG)
    }
}
