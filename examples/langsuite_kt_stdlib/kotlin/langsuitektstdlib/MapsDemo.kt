// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * `Map` idioms over `MapsKt` and the `HashMap` builtin (`LinkedHashMap` is an alias, so nothing
 * here depends on insertion order).
 */
object MapsDemo {
    private const val TAG = "MapsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Any?): Any? = v

    fun run() {
        Log.i(TAG, "=== Maps Tests ===")

        val ages = mapOf("ann" to 31, "bob" to 25, "cy" to 40)
        check("mapOf size", ages.size == 3)
        check("get / index", ages["bob"] == 25 && ages.get("cy") == 40 && ages["zed"] == null)
        check("containsKey / in", "ann" in ages && "zed" !in ages && ages.containsKey("cy"))
        check("containsValue", ages.containsValue(40) && !ages.containsValue(1))
        check(
            "getOrDefault / getOrElse",
            ages.getOrDefault("zed", 0) == 0 &&
                ages.getOrElse("zed") { -1 } == -1 &&
                ages.getOrElse("ann") { -1 } == 31,
        )
        check("getValue", ages.getValue("ann") == 31)
        check(
            "getValue missing throws",
            try {
                ages.getValue("zed")
                false
            } catch (e: NoSuchElementException) {
                true
            },
        )
        check("isEmpty / isNotEmpty", ages.isNotEmpty() && emptyMap<String, Int>().isEmpty())
        var total = 0
        for ((_, v) in ages) total += v
        check("for ((k, v) in map)", total == 96)
        var keyLen = 0
        ages.forEach { (k, v) -> keyLen += k.length + v }
        check("forEach destructuring", keyLen == 8 + 96)
        var n = 0
        for (e in ages.entries) {
            n += e.key.length
            n += e.value
        }
        check("entries", n == 104)
        check(
            "keys / values",
            ages.keys.size == 3 &&
                ages.values.sum() == 96 &&
                "bob" in ages.keys &&
                ages.keys.contains("ann"),
        )
        check("keys sorted", ages.keys.sorted().joinToString() == "ann, bob, cy")
        check("values max", ages.values.max() == 40 && ages.values.maxOrNull() == 40)
        check("map {}", ages.map { (k, v) -> "$k=$v" }.sorted().first() == "ann=31")
        check(
            "filter",
            ages.filter { it.value > 30 }.size == 2 &&
                ages.filter { it.key == "bob" }.keys.first() == "bob",
        )
        check(
            "filterKeys / filterValues",
            ages.filterKeys { it.length == 2 }.size == 1 && ages.filterValues { it < 30 }.size == 1,
        )
        check(
            "mapValues / mapKeys",
            ages.mapValues { it.value + 1 }["ann"] == 32 &&
                ages.mapKeys { it.key.uppercase() }["CY"] == 40,
        )
        check(
            "any / all / none / count",
            ages.any { it.value > 39 } &&
                ages.all { it.value > 20 } &&
                ages.none { it.key == "z" } &&
                ages.count { it.value > 30 } == 2,
        )
        check(
            "maxByOrNull / minByOrNull",
            ages.maxByOrNull { it.value }?.key == "cy" &&
                ages.minByOrNull { it.value }!!.key == "bob",
        )
        check(
            "entries sortedBy",
            ages.entries.sortedBy { it.value }.map { it.key }.joinToString() == "bob, ann, cy",
        )
        check(
            "toList",
            ages.toList().size == 3 && ages.toList().sortedBy { it.second }.first().first == "bob",
        )

        val mm = mutableMapOf<String, Int>()
        mm["a"] = 1
        mm.put("b", 2)
        mm["a"] = mm["a"]!! + 10
        check("mutableMapOf put / update", mm.size == 2 && mm["a"] == 11)
        mm += "c" to 3
        check("plusAssign pair", mm["c"] == 3)
        for ((k, v) in mapOf("d" to 4, "e" to 5)) mm[k] = v
        check("put from another map", mm.size == 5)
        check("remove", mm.remove("b") == 2 && mm.size == 4 && mm.remove("zz") == null)
        check(
            "getOrPut",
            mm.getOrPut("f") { 6 } == 6 && mm.getOrPut("f") { 99 } == 6 && mm.size == 5,
        )
        mm.clear()
        check("clear", mm.isEmpty())
        val counts = HashMap<String, Int>()
        for (w in "a b a c b a".split(" ")) counts[w] = (counts[w] ?: 0) + 1
        check("word count idiom", counts["a"] == 3 && counts["b"] == 2 && counts["c"] == 1)
        val plus = ages + ("dee" to 1)
        check("plus pair (copy)", plus.size == 4 && ages.size == 3)
        check("plus map", (ages + mapOf("x" to 1)).size == 4)
        check("minus key", (ages - "ann").size == 2)
        val copy = ages.toMutableMap()
        copy["ann"] = 0
        check("toMutableMap copy", copy["ann"] == 0 && ages["ann"] == 31)
        check("toMap", copy.toMap()["cy"] == 40)
        check(
            "hashMapOf / linkedMapOf",
            hashMapOf(1 to "x").size == 1 && linkedMapOf("k" to "v")["k"] == "v",
        )
        check("emptyMap", emptyMap<String, Int>().size == 0 && mapOf<Int, Int>().isEmpty())
        check("mapOf single", mapOf("only" to 1).size == 1)
        val nested = mapOf("l" to listOf(1, 2))
        check("nested list values", nested["l"]!!.sum() == 3)
        val intKeys = mapOf(1 to "one", 2 to "two")
        check("Int keys", intKeys[2] == "two" && intKeys[3] == null)
        val grouped = listOf("aa", "b", "cc").groupBy { it.length }
        check("groupBy sizes", grouped[2]!!.size == 2 && grouped.getValue(1).first() == "b")
        check("map template", "${ages["ann"]}" == "31")
        check("keys as set ops", (ages.keys - "ann").size == 2 && ages.keys.toList().size == 3)
        check(
            "mapNotNull on map",
            ages.mapNotNull { if (it.value > 30) it.key else null }.size == 2,
        )
        check(
            "map to sorted list of pairs",
            ages.toList().sortedByDescending { it.second }.first().first == "cy",
        )
        check("entry destructuring in map {}", ages.map { (k, _) -> k.length }.sum() == 8)
        check(
            "mutable iteration + put",
            run {
                val m = mutableMapOf(1 to 1)
                for (k in m.keys.toList()) m[k + 1] = 2
                m.size == 2
            },
        )
        check(
            "map with null values",
            mapOf("k" to null).containsKey("k") &&
                mapOf("k" to opaque(null))["k"] == null &&
                mapOf("k" to opaque(null)).size == 1,
        )
        check("Pair keys unsupported avoided; Char keys", mapOf('a' to 1)['a'] == 1)
        check("count on map", ages.count() == 3)
        check("sumOf on map", ages.entries.sumOf { it.value } == 96)
        check(
            "map.iterator explicit",
            run {
                val it2 = ages.iterator()
                var c = 0
                while (it2.hasNext()) {
                    it2.next()
                    c++
                }
                c == 3
            },
        )

        // S9: emptyMap() must not be a shared mutable singleton.
        @Suppress("UNCHECKED_CAST") (emptyMap<String, Int>() as MutableMap<String, Int>).put("x", 1)
        check("emptyMap not shared", emptyMap<String, Int>().isEmpty())

        Check.done(TAG)
    }
}
