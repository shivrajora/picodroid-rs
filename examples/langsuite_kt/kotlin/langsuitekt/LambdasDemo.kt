// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Lambdas and function values: `Function0..4` through `invokedynamic`, boxing across `invoke`,
 * captured mutable locals (`Ref.IntRef` and friends), SAM conversion, `fun interface`, lambdas
 * returning lambdas, and `Unit`.
 */
object LambdasDemo {
    private const val TAG = "LambdasKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    fun interface IntPred {
        fun test(x: Int): Boolean
    }

    class Holder(val f: (Int) -> Int) {
        fun apply(x: Int): Int = f(x)
    }

    private fun apply3(f: (Int) -> Int, x: Int): Int = f(f(f(x)))

    private fun sum3(f: (Int, Int, Int) -> Int): Int = f(1, 2, 3)

    private fun sum4(f: (Int, Int, Int, Int) -> Int): Int = f(1, 2, 3, 4)

    fun run() {
        Log.i(TAG, "=== Lambda Tests ===")

        val double: (Int) -> Int = { it * 2 }
        check("Function1 boxing", double(21) == 42)
        val add: (Int, Int) -> Int = { a, b -> a + b }
        check("Function2", add(20, 22) == 42)
        val greet: () -> String = { "hi" }
        check("Function0", greet() == "hi")
        check("Function3", sum3 { a, b, c -> a + b + c } == 6)
        check("Function4", sum4 { a, b, c, d -> a * b * c * d } == 24)

        var count = 0
        val inc: () -> Unit = { count++ }
        inc()
        inc()
        check("captured var (Ref.IntRef)", count == 2)
        val u = inc()
        check("Unit lambda returns Unit", u == Unit)

        var label: String? = null
        val setLabel: (String) -> Unit = { label = it }
        setLabel("x")
        check("captured var (Ref.ObjectRef)", label == "x")

        var total = 0L
        val addL: (Long) -> Unit = { total += it }
        addL(5L)
        addL(7L)
        check("captured var (Ref.LongRef)", total == 12L)

        var acc = 0f
        val addF: (Float) -> Unit = { acc += it }
        addF(1.5f)
        check("captured var (Ref.FloatRef)", acc == 1.5f)

        var ran = false
        Runnable { ran = true }.run()
        check("SAM conversion + Ref.BooleanRef", ran)

        check("higher-order", apply3({ it + 1 }, 0) == 3)
        val even = IntPred { it % 2 == 0 }
        check("fun interface", even.test(4) && !even.test(3))
        val h = Holder { it - 1 }
        check("lambda in field", h.apply(10) == 9)

        val base = 100
        val addBase: (Int) -> Int = { it + base }
        check("captured val", addBase(1) == 101)
        val nested: (Int) -> (Int) -> Int = { a -> { b -> a * b } }
        check("lambda returning lambda", nested(6)(7) == 42)

        val strs = arrayOf("a", "bb", "ccc")
        var lens = 0
        val each: (String) -> Unit = { lens += it.length }
        for (s in strs) each(s)
        check("lambda over array", lens == 6)

        Check.done(TAG)
    }
}
