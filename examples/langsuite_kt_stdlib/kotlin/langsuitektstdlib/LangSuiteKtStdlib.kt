// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.app.Application
import picodroid.util.Log

/**
 * The stdlib half of the Kotlin conformance suite: collections, arrays, maps, sets, ranges,
 * strings, math and sorting — the `kotlin-shim` tiers 1–2 surface plus the inline HOFs over the
 * JVM's builtin collections. It is a separate app from `langsuite_kt` (the language half) because
 * the two together parse ~155 classes, whose metadata alone exceeds the 416 KB heap arena the sim
 * and the RP2350 boards share (the census: ~1.5 KB per parsed class on the 64-bit sim). Both apps
 * are fixtures of `:kotlin-shim:contractCheck`.
 */
class LangSuiteKtStdlib : Application() {
    override fun onCreate() {
        Log.i(TAG, "=== LangSuiteKtStdlib start ===")

        safe("collections") { CollectionsDemo.run() }
        safe("arrays") { ArraysDemo.run() }
        safe("maps") { MapsDemo.run() }
        safe("sets") { SetsDemo.run() }
        safe("ranges") { RangesDemo.run() }
        safe("strings") { StringsDemo.run() }
        safe("math") { MathDemo.run() }
        safe("sorting") { SortingDemo.run() }

        Log.i(TAG, "=== LangSuiteKtStdlib done ===")
    }

    /** Not inline on purpose: the demo runs through a real `Function0`. */
    private fun safe(name: String, demo: () -> Unit) {
        try {
            demo()
        } catch (t: Throwable) {
            Log.i(TAG, "$name threw: $t")
        }
    }

    companion object {
        private const val TAG = "LangSuiteKtStdlib"
    }
}

/** Shared pass/fail tally; each demo calls [done] under its own tag. */
object Check {
    private var passed = 0
    private var failed = 0

    fun check(tag: String, name: String, ok: Boolean) {
        if (ok) {
            Log.i(tag, "PASS: $name")
            passed++
        } else {
            Log.i(tag, "FAIL: $name")
            failed++
        }
    }

    fun done(tag: String) {
        Log.i(tag, "Results: $passed passed, $failed failed")
        Log.i(tag, if (failed == 0) "=== ALL PASSED ===" else "=== SOME FAILED ===")
        passed = 0
        failed = 0
    }
}
