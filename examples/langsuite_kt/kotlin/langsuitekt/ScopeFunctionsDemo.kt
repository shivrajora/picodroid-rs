// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * `let` / `apply` / `also` / `run` / `with` / `takeIf` / `takeUnless` — all inline; no shim call.
 */
object ScopeFunctionsDemo {
    private const val TAG = "ScopeKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Any?): Any? = v

    private class Box(var v: Int = 0, var label: String = "")

    fun run() {
        Log.i(TAG, "=== Scope Function Tests ===")

        check("let", "abc".let { it.length + 1 } == 4)
        check(
            "let on nullable",
            (opaque(null) as String?)?.let { it.length } == null &&
                (opaque("xy") as String?)?.let { it.length } == 2,
        )
        check("let ?: default", ((opaque(null) as String?)?.let { it.length } ?: -1) == -1)
        val b =
            Box().apply {
                v = 5
                label = "five"
            }
        check("apply", b.v == 5 && b.label == "five")
        var sideEffect = 0
        val same = b.also { sideEffect = it.v }
        check("also", same === b && sideEffect == 5)
        check("run", b.run { v * 2 } == 10 && run { 1 + 2 } == 3)
        check("with", with(b) { "$label=$v" } == "five=5")
        check(
            "takeIf / takeUnless",
            7.takeIf { it > 5 } == 7 &&
                3.takeIf { it > 5 } == null &&
                3.takeUnless { it > 5 } == 3 &&
                "x".takeIf { it.isEmpty() } == null,
        )
        check(
            "chained",
            "  hello  "
                .trim()
                .let { it.uppercase() }
                .also { sideEffect = it.length }
                .takeIf { it.startsWith("H") } == "HELLO" && sideEffect == 5,
        )
        val list =
            mutableListOf<Int>()
                .apply {
                    add(1)
                    add(2)
                }
                .also { it.add(3) }
        check("apply on collection", list.size == 3 && list.sum() == 6)
        check(
            "run returning Unit",
            run {
                sideEffect = 9
                Unit
            } == Unit && sideEffect == 9,
        )
        check("let with destructuring", (1 to 2).let { (a, c) -> a + c } == 3)
        check("nested scope this/it", b.apply { v = 1 }.let { it.v + b.run { v } } == 2)
        check(
            "takeIf chain null-safe",
            (opaque(4) as Int?)?.takeIf { it % 2 == 0 }?.let { it * 10 } == 40,
        )
        check("with on list", with(listOf(1, 2, 3)) { size + first() + last() } == 7)
        check(
            "apply returns receiver",
            StringBuilder().apply { append("a") }.apply { append("b") }.toString() == "ab",
        )
        check("let on Int", 5.let { it * it } == 25)
        check("run with receiver", "kt".run { length } == 2)
        check("also returns receiver value", 3.also { sideEffect = it } == 3 && sideEffect == 3)
        check(
            "repeat with scope",
            run {
                var s = 0
                repeat(4) { s += it }
                s
            } == 6,
        )
        check("let returning Pair", "a".let { it to it.length }.second == 1)
        check(
            "apply with when",
            Box()
                .apply {
                    v =
                        when (label) {
                            "" -> 1
                            else -> 2
                        }
                }
                .v == 1,
        )
        check("also side-effect list", mutableListOf<String>().also { it.add("z") }.first() == "z")
        val fn: (Int) -> Int = { it + 1 }
        check("lambda value + let", 1.let(fn) == 2)
        check(
            "error path through let",
            try {
                "x".let { it.toInt() }
                false
            } catch (e: NumberFormatException) {
                true
            },
        )
        check("takeIf on object", b.takeIf { it.v > 0 }?.label == "five")
        check(
            "let with label return",
            listOf(1, 2).let l@{
                if (it.isEmpty()) return@l -1
                it.sum()
            } == 3,
        )
        check("apply on nullable via ?.", (opaque(Box()) as Box?)?.apply { v = 3 }?.v == 3)
        check("with returning Unit", with(b) { v = 2 } == Unit && b.v == 2)

        Check.done(TAG)
    }
}
