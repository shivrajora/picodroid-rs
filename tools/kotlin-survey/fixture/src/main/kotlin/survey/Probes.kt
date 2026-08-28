// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.concurrent.Executors
import picodroid.util.Log

/**
 * Deliberately out-of-scope or undecided shapes, one probe function each, so
 * the dump's `from_member` column names the probe. This is the ONLY fixture
 * file allowed to use `::`, `::class`, `uppercase`/`lowercase`, the no-arg
 * `mutableMapOf`/`mutableSetOf`/`hashMapOf`, `toTypedArray`, `println`, or
 * `is MutableList`; the inventory's tier tables exclude `Probes.kt`.
 */
object Probes {
    private const val TAG = "Probes"

    fun twice(n: Int): Int = n * 2

    /** (a) callable reference to a Kotlin function: indy or FunctionReferenceImpl subclass? */
    fun probeA_functionReference(): Int {
        val f: (Int) -> Int = ::twice
        return f(1) + listOf(1, 2).map(::twice).sum()
    }

    /** (b) references to JDK/builtin members and a constructor reference (REF_newInvokeSpecial). */
    fun probeB_builtinReferences(names: List<String>, readings: List<Reading>): Int {
        val lens = names.map(String::length)
        val vals = readings.map(Reading::value)
        val ths = listOf(1f, 2f).map(::Threshold)
        // The same references outside an inline HOF: what class do they become?
        val lenRef: (String) -> Int = String::length
        val valRef: (Reading) -> Float = Reading::value
        val ctorRef: (Float) -> Threshold = ::Threshold
        return lens.size + vals.size + ths.size + lenRef("x") + valRef(readings[0]).toInt() + ctorRef(1f).v.toInt()
    }

    /** (c) a Serializable SAM forces altMetafactory. */
    fun interface SerialSam : java.io.Serializable {
        fun go(): Int
    }

    fun probeC_altMetafactory(): Int {
        val s = SerialSam { 1 }
        return s.go()
    }

    /** (e) `uppercase()`/`lowercase()` → `toUpperCase(Locale.ROOT)`? */
    fun probeE_case(s: String): String = s.uppercase() + s.lowercase()

    /** (f) no-arg builders → `new java/util/LinkedHashMap` / `LinkedHashSet` at the call site? */
    fun probeF_builders(): Int {
        val m = mutableMapOf<String, Int>()
        m["a"] = 1
        val s = mutableSetOf<Int>()
        s += 2
        val h = hashMapOf<String, Int>()
        h["b"] = 3
        return m.size + s.size + h.size
    }

    /** (k) crossinline in an object expression, and plain `() -> Unit` values. */
    inline fun runLater(crossinline f: () -> Unit) {
        Executors.mainExecutor().execute(
            object : Runnable {
                override fun run() = f()
            },
        )
    }

    fun call(f: () -> Unit) = f()

    fun probeK_lambdaClasses() {
        runLater { Log.d(TAG, "later") }
        val g: () -> Unit = { Log.d(TAG, "g") }
        call(g)
        call { Log.d(TAG, "call-site lambda") }
    }

    /** (m) `toTypedArray()` → `Collection.toArray`? */
    fun probeM_toTypedArray(): Int = listOf("a").toTypedArray().size

    /** Reflection shapes the contract test must reject. */
    fun probeR_reflection(x: Any): String = Reading::class.simpleName + (x is MutableList<*>) + Registry::created.name

    /** `println` → `kotlin/io/ConsoleKt` → System.out (absent on pico-jvm). */
    fun probeP_println() {
        println("x")
        println(42)
    }
}
