// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * `lazy { }` (once, `isInitialized`, `toString`) and `Pair` / `to` (accessors, destructuring,
 * equality, templates).
 */
object LazyPairDemo {
    private const val TAG = "LazyPairKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private var inits = 0
    private val heavy: Int by lazy {
        inits++
        42
    }
    private val raw = lazy { "computed" }

    fun run() {
        Log.i(TAG, "=== Lazy / Pair Tests ===")

        check("lazy not computed yet", inits == 0)
        check("lazy value", heavy == 42)
        check("lazy once", heavy == 42 && inits == 1)
        check("Lazy.isInitialized false", !raw.isInitialized())
        check("Lazy toString before", "$raw" == "Lazy value not initialized yet.")
        check("Lazy.value", raw.value == "computed")
        check("Lazy.isInitialized true", raw.isInitialized())
        check("Lazy toString after", "$raw" == "computed")

        val p = 1 to "a"
        check("to", p.first == 1 && p.second == "a")
        val (n, s) = p
        check("destructuring", n == 1 && s == "a")
        check("Pair toString", p.toString() == "(1, a)")
        check("Pair equals", p == (1 to "a"))
        check("Pair not equals", p != (2 to "a"))
        check("Pair hashCode", p.hashCode() == (1 to "a").hashCode())
        check("Pair template", "$p" == "(1, a)")
        val nested = (1 to 2) to 3
        check("nested Pair", nested.first.second == 2 && nested.second == 3)
        val pairs = arrayOf(1 to "x", 2 to "yy")
        var total = 0
        for ((k, v) in pairs) total += k + v.length
        check("destructuring in for", total == 6)
        val nullable: Pair<String?, Int> = null to 0
        check("Pair with null", nullable.first == null && "$nullable" == "(null, 0)")

        Check.done(TAG)
    }
}
