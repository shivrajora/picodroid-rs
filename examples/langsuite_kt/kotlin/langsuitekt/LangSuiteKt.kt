// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.app.Application
import picodroid.util.Log

/**
 * The Kotlin twin of `langsuite`, language half (lambdas, null safety, objects, data classes,
 * enums, sealed/when, interface defaults, exceptions, lazy/Pair, scope functions, default
 * arguments, varargs, extensions/operators, synchronized, type checks); the stdlib half is
 * `langsuite_kt_stdlib`, split off so each app's parsed-class metadata fits the 416 KB heap arena.
 * Every sub-demo is a self-checking `object` printing `=== ALL PASSED ===` under its own tag. Both
 * apps are the fixtures the kotlin-shim contract check runs against — every `kotlin/…` reference
 * they make must resolve in `kotlin-shim/`.
 */
class LangSuiteKt : Application() {
    override fun onCreate() {
        Log.i(TAG, "=== LangSuiteKt start ===")

        safe("lambdas") { LambdasDemo.run() }
        safe("nullsafety") { NullSafetyDemo.run() }
        safe("objects") { ObjectsDemo.run() }
        safe("dataclass") { DataClassDemo.run() }
        safe("enum") { EnumDemo.run() }
        safe("sealedwhen") { SealedWhenDemo.run() }
        safe("interfacedefault") { InterfaceDefaultDemo.run() }
        safe("exceptions") { ExceptionsDemo.run() }
        safe("lazypair") { LazyPairDemo.run() }
        safe("scope") { ScopeFunctionsDemo.run() }
        safe("defaultargs") { DefaultArgsDemo.run() }
        safe("varargs") { VarargsDemo.run() }
        safe("extension") { ExtensionDemo.run() }
        safe("sync") { SyncDemo.run() }
        safe("typechecks") { TypeChecksDemo.run() }

        Log.i(TAG, "=== LangSuiteKt done ===")
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
        private const val TAG = "LangSuiteKt"
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
