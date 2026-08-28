// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.app.Activity
import picodroid.util.Log
import picodroid.widget.ListView
import picodroid.widget.TextView

/**
 * Activity 2 (HistoryActivity shape): every range form in and out of `for`, `withIndex`,
 * `coerceIn`, `repeat`, `buildString`, `String.format`, try/catch/finally, `synchronized`, and
 * `error()` behind an elvis.
 */
class HistoryActivity : Activity() {
    private val lock = Any()
    private val samples = FloatArray(60)
    private var parsedCount = 0
    private var done = false
    private var service: SensorService? = null

    override fun onCreate() {
        super.onCreate()
        val n = fill()
        val list = ListView()
        for (i in 0 until n) list.addItem(samples[i].fmt1())
        for (i in samples.indices) samples[i] = samples[i] * 0.5f
        for (i in n - 1 downTo 0 step 2) Log.d(TAG, "even-from-top $i=${samples[i]}")
        for (i in 1..10) samples[i] = i.toFloat()
        for ((i, v) in samples.withIndex()) if (v > 100f) Log.w(TAG, "spike at $i: $v")

        val idx = intent.getIntExtra("idx", 0)
        if (idx in 0..samples.lastIndex) Log.i(TAG, "idx ok $idx")
        val pos = idx.coerceIn(0, n - 1)
        repeat(3) { Log.d(TAG, "tick $it") }
        val summary = buildString {
            append("n=").append(n)
            append(" pos=").append(pos)
        }
        val header = TextView()
        header.setText(String.format("%.1f (%s)", samples[pos], summary))
        setContentView(header)

        val parsed =
            try {
                intent.getStringExtra("n").toInt()
            } catch (e: NumberFormatException) {
                Log.w(TAG, "bad n: ${e.message}")
                -1
            } finally {
                done = true
            }
        synchronized(lock) { parsedCount = parsed }
        val svc = service ?: error("no service bound")
        Log.i(TAG, "count=$parsedCount done=$done latest=${svc.latest()}")
    }

    private fun fill(): Int {
        var n = 0
        while (n < samples.size / 2) {
            samples[n] = n * 1.5f
            n++
        }
        return n
    }

    companion object {
        private const val TAG = "History"
    }
}
