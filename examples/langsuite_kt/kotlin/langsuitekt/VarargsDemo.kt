// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * `vararg` parameters and the spread operator (`Arrays.copyOf`, `SpreadBuilder` for mixed calls).
 */
object VarargsDemo {
    private const val TAG = "VarargsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun sum(vararg xs: Int): Int {
        var s = 0
        for (x in xs) s += x
        return s
    }

    private fun join(vararg parts: String, sep: String = "+") = parts.joinToString(sep)

    private fun count(vararg any: Any?) = any.size

    private fun firstOrZero(vararg fs: Float) = if (fs.isEmpty()) 0f else fs[0]

    private fun <T> listFrom(vararg items: T): List<T> = listOf(*items)

    private fun sumLongs(vararg ls: Long) = ls.sum()

    private fun describe(prefix: String, vararg tail: Int) = prefix + tail.joinToString("")

    private fun passThrough(vararg xs: Int) = sum(*xs)

    private fun keep(vararg xs: Int): IntArray = xs

    fun run() {
        Log.i(TAG, "=== Varargs Tests ===")

        check("no args", sum() == 0 && count() == 0 && join() == "")
        check("some args", sum(1, 2, 3) == 6 && join("a", "b") == "a+b")
        check("named after vararg", join("a", "b", sep = "-") == "a-b")
        val arr = intArrayOf(4, 5)
        check("spread IntArray", sum(*arr) == 9)
        check(
            "mixed spread IntArray",
            sum(1, *arr) == 10 && sum(*arr, *arr) == 18 && sum(*arr, 7) == 16,
        )
        val strs = arrayOf("x", "y")
        check("spread Array<String>", join(*strs) == "x+y")
        check("mixed spread Array<String>", join("w", *strs, "z") == "w+x+y+z")
        check("mixed Any", count(1, "s", null, 2.5) == 4)
        check("float vararg", firstOrZero() == 0f && firstOrZero(1.5f, 2f) == 1.5f)
        check(
            "generic vararg to listOf",
            listFrom("a", "b").size == 2 && listFrom<Int>().isEmpty() && listFrom(1, 2, 3)[2] == 3,
        )
        check("long vararg", sumLongs(1L, 2L) == 3L)
        check("vararg after fixed", describe("n:", 1, 2) == "n:12" && describe("n:") == "n:")
        check("vararg is array", sum(*IntArray(3) { it }) == 3)
        check("spread a list via toTypedArray", join(*listOf("p", "q").toTypedArray()) == "p+q")
        check("arrayOf then spread into listOf", listOf(*arrayOf(1, 2)).sum() == 3)
        check("vararg passed through", passThrough(7, 8) == 15)
        check(
            "spread copies",
            run {
                val a = intArrayOf(1)
                val f = keep(*a)
                a[0] = 9
                f[0] == 1
            },
        )
        check("vararg in lambda", listOf(1, 2).map { sum(it, it) }.sum() == 6)
        check(
            "listOf / setOf / mapOf varargs",
            listOf(1, 2, 3).size == 3 && setOf(1, 1).size == 1 && mapOf(1 to 1, 2 to 2).size == 2,
        )
        check("intArrayOf empty", intArrayOf().isEmpty() && arrayOf<String>().isEmpty())
        check(
            "spread into intArrayOf-free: copyOf",
            arr.copyOf(3).size == 3 && arr.copyOf(3)[2] == 0,
        )

        Check.done(TAG)
    }
}
