// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.util.Log

/** The collection surface a picoenvmon-sized app uses: builders, inline HOFs, maps, destructuring, sorting. */
object CollectionsDemo {
    private const val TAG = "Collections"

    fun run(readings: List<Reading>): String {
        val ints = listOf(1, 2, 3)
        val one = listOf("a")
        val mutable = mutableListOf<Reading>()
        val map = mapOf("t" to 1, "h" to 2)
        val mmap = mutableMapOf("k" to 0)
        val set = setOf(1, 2)

        val doubled = ints.map { it * 2 }
        val big = readings.filter { it.value > 10f }
        readings.forEach { Log.d(TAG, "r $it") }
        readings.forEachIndexed { i, r -> Log.d(TAG, "$i: ${r.value}") }
        val anyHot = readings.any { it.value > 30f }
        val hot = readings.count { it.value > 30f }
        val totalType = readings.sumOf { it.type }
        val totalValue = readings.sumOf { it.value.toDouble() }
        val joined = readings.joinToString(", ") { it.value.toString() }
        val plainJoined = ints.joinToString()
        val byTs = readings.sortedBy { it.ts }
        val maxValue = readings.map { it.value }.maxOrNull()
        val first = readings.first()
        val firstLight = readings.firstOrNull { it.type == 5 }
        val head = readings.take(3)
        val zipped = ints.zip(one)
        for ((i, r) in readings.withIndex()) Log.d(TAG, "$i=$r")
        val distinctTypes = readings.map { it.type }.distinct()

        for ((k, v) in map) Log.d(TAG, "$k=$v")
        map.forEach { (k, v) -> Log.d(TAG, "$k->$v") }
        val (key, value) = "x" to 1
        val firstPlusOne = ints[0] + 1
        val identitySum = ints.sumOf { it }
        mutable += first
        mmap["z"] = 3
        val got = mmap.getOrPut("q") { 0 }
        val keys = map.keys
        val values = map.values
        val arr = ints.toIntArray()
        val byTsCmp = readings.sortedWith(compareBy { it.ts })
        val byType = readings.sortedWith(Comparator { x, y -> x.type - y.type })
        val empty = ints.isEmpty() && mutable.isEmpty() && set.isEmpty()
        val contained = 2 in set && "t" in map

        return "doubled=$doubled big=${big.size} anyHot=$anyHot hot=$hot totalType=$totalType totalValue=$totalValue" +
            " joined=$joined plain=$plainJoined byTs=${byTs.size} max=$maxValue first=$first firstLight=$firstLight" +
            " head=${head.size} zipped=$zipped distinct=$distinctTypes key=$key value=$value +1=$firstPlusOne" +
            " idSum=$identitySum got=$got keys=${keys.size} values=${values.size} arr=${arr.size}" +
            " byTsCmp=${byTsCmp.size} byType=${byType.size} empty=$empty contained=$contained mmap=${mmap.size}"
    }
}
