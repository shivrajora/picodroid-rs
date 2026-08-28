// SPDX-License-Identifier: GPL-3.0-only
package hellokt

import picodroid.app.Application
import picodroid.os.SystemClock
import picodroid.util.Log

/**
 * The smallest Kotlin app: a string template, one `!!` (the `Intrinsics.checkNotNull` call the shim
 * serves), and a SAM lambda for a Java interface (an `invokedynamic` the JVM already runs for Java
 * apps).
 */
class HelloKt : Application() {
    override fun onCreate() {
        Log.i(TAG, "hi from kotlin ${21 * 2}")
        val maybe: String? = if (SystemClock.elapsedRealtimeNanos() >= 0) "not null" else null
        Log.i(TAG, "bang ${maybe!!}")
        Runnable { Log.i(TAG, "lambda ran") }.run()
    }

    companion object {
        private const val TAG = "HelloKt"
    }
}
