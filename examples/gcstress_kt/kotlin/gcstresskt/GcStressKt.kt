// SPDX-License-Identifier: GPL-3.0-only
package gcstresskt

import java.util.ArrayList
import java.util.HashMap
import picodroid.app.Application
import picodroid.os.Runtime
import picodroid.os.SystemClock
import picodroid.util.Log

private const val TAG = "GcStressKt"

/**
 * The Kotlin twin of `examples/gcstress` (roadmap Session 8): the same mark-sweep pressure, but
 * through the short-lived objects Kotlin codegen mints that Java's never does — `invokedynamic`
 * lambda proxies per loop iteration, `Ref$IntRef` boxes for captured-and-mutated locals, `Integer`
 * autoboxing through generic collections, `Pair` allocation + destructuring, string templates, and
 * map iteration through entry views (the Session 6 iterator-pins-its-source GC fix, under churn).
 *
 * Two assertions the Java app cannot express:
 * - a live object's identity `hashCode()` (its heap slot index) must be stable across churn that
 *   reuses every dead slot around it;
 * - objects whose only reference is a lambda capture must survive collections (capture rooting).
 *
 * Unlike `picoenvmon_kt` this app uses the stdlib idioms on purpose — churn is the point — so its
 * PAPK carries shim collections/tuples classes. Every phase verifies its arithmetic; any mismatch
 * logs `FAIL:` and the final line becomes `=== FAILED ===` (the HIL/sim lanes grep for `=== PASSED
 * ===`).
 */
class GcStressKt : Application() {
    private var sinkInt = 0
    private var sink: Any? = null
    private var retained: Node? = null
    private var ok = true

    override fun onCreate() {
        Log.i(TAG, "=== GC Stress Test (Kotlin) ===")
        var total = 0L
        var t: Long

        t = stressLambdaChurn()
        total += t
        report("lambda_churn", t)

        t = stressRefCapture()
        total += t
        report("ref_capture", t)

        t = stressCaptureRooting()
        total += t
        report("capture_rooting", t)

        t = stressBoxingList()
        total += t
        report("boxing_list", t)

        t = stressPairChurn()
        total += t
        report("pair_churn", t)

        t = stressStringTemplate()
        total += t
        report("string_template", t)

        t = stressMapEntries()
        total += t
        report("map_entries", t)

        t = stressSlotHashCode()
        total += t
        report("slot_hashcode", t)

        t = stressRetention()
        total += t
        report("retention", t)

        Log.i(TAG, "TOTAL: ${total / 1000L} us")
        if (ok) {
            Log.i(TAG, "=== PASSED ===")
        } else {
            Log.i(TAG, "=== FAILED ===")
        }
    }

    private fun fail(msg: String) {
        ok = false
        Log.i(TAG, "FAIL: $msg")
    }

    private fun report(name: String, wallNs: Long) {
        val wallUs = wallNs / 1000L
        val gcUs = Runtime.gcTimeNanos() / 1000L
        val gcCount = Runtime.gcCount()
        val gcFreed = Runtime.gcFreed()
        Log.i(TAG, "$name: $wallUs us (gc: $gcUs us, $gcCount collections, $gcFreed freed)")
    }

    // ── 1. Lambda churn ──────────────────────────────────────────────────────
    // A capturing lambda is a fresh proxy object per iteration, and `f()` on a
    // `() -> Int` returns a boxed Integer: two short-lived allocations a loop.

    private fun stressLambdaChurn(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        var sum = 0
        for (i in 0 until 2048) {
            val f = { i * 2 + 1 }
            sum += f()
        }
        sinkInt = sum
        if (sum != 2048 * 2048) {
            fail("lambda_churn sum=$sum")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 2. Ref-box churn ─────────────────────────────────────────────────────
    // A local that a lambda mutates is hoisted into a `Ref$IntRef` box — one
    // per iteration here, plus the lambda that writes through it.

    private fun stressRefCapture(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        var sum = 0
        for (i in 0 until 1024) {
            var local = i
            val bump = { local += 3 }
            bump()
            sum += local
        }
        sinkInt = sum
        if (sum != 526848) {
            fail("ref_capture sum=$sum")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 3. Capture rooting ───────────────────────────────────────────────────
    // Each Supplier proxy is the ONLY reference to its Node. 2048 allocations
    // of churn force collections in between; the captures must survive.

    private fun stressCaptureRooting(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        val suppliers = arrayOfNulls<Supplier>(8)
        for (j in 0 until 8) {
            val n = Node(j * 11)
            suppliers[j] = Supplier { n.value }
        }
        var last: Node? = null
        for (i in 0 until 2048) {
            last = Node(i)
        }
        sink = last
        var sum = 0
        for (j in 0 until 8) {
            sum += suppliers[j]!!.get()
        }
        if (sum != 308) {
            fail("capture_rooting sum=$sum")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 4. Boxing through a generic list ─────────────────────────────────────
    // `mutableListOf<Int>()` is a plain ArrayList; every add boxes, every
    // index read unboxes through a checkcast.

    private fun stressBoxingList(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        var sum = 0
        for (i in 0 until 512) {
            val list = ArrayList<Int>()
            list.add(i)
            list.add(i + 1)
            list.add(i + 2)
            list.add(i + 3)
            sum += list[0] + list[3]
        }
        sinkInt = sum
        if (sum != 263168) {
            fail("boxing_list sum=$sum")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 5. Pair churn ────────────────────────────────────────────────────────
    // `a to b` allocates a Pair plus two Integer boxes; destructuring calls
    // component1/component2 and unboxes both.

    private fun stressPairChurn(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        var sum = 0
        for (i in 0 until 1024) {
            val p = i to i + 1
            val (a, b) = p
            sum += b - a
        }
        sinkInt = sum
        if (sum != 1024) {
            fail("pair_churn sum=$sum")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 6. String-template churn ─────────────────────────────────────────────

    private fun stressStringTemplate(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        var lastLen = 0
        for (i in 0 until 1024) {
            val s = "gc$i:${i * 2}"
            lastLen = s.length
        }
        sinkInt = lastLen
        if (lastLen != 11) {
            fail("string_template lastLen=$lastLen")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 7. Map entry-view churn ──────────────────────────────────────────────
    // `for ((k, v) in m)` walks entrySet() through an iterator whose source
    // must stay pinned (the Session 6 GC fix) while entry objects churn.

    private fun stressMapEntries(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        var sum = 0
        for (i in 0 until 256) {
            val m = HashMap<Int, Int>()
            for (j in 0 until 4) {
                m[j] = i + j
            }
            for ((k, v) in m) {
                sum += v - k
            }
        }
        sinkInt = sum
        if (sum != 130560) {
            fail("map_entries sum=$sum")
        }
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 8. Identity hashCode across slot reuse ───────────────────────────────
    // Identity hashCode is the heap slot index (documented divergence). A LIVE
    // object's slot is never reused, so its hashCode must not move no matter
    // how many dead slots churn around it.

    private fun stressSlotHashCode(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        val anchor = Node(4242)
        retained = anchor
        val h0 = anchor.hashCode()
        for (i in 0 until 2048) {
            sink = Node(i)
        }
        val h1 = anchor.hashCode()
        if (h0 != h1) {
            fail("slot_hashcode moved $h0 -> $h1")
        }
        retained = null
        return SystemClock.elapsedRealtimeNanos() - start
    }

    // ── 9. Retention pattern (same as the Java app) ──────────────────────────

    private fun stressRetention(): Long {
        Runtime.resetGcStats()
        val start = SystemClock.elapsedRealtimeNanos()
        for (i in 0 until 1024) {
            val n = Node(i)
            if (i % 100 == 0) {
                retained = n
            }
        }
        sinkInt = retained!!.value
        retained = null
        return SystemClock.elapsedRealtimeNanos() - start
    }
}
