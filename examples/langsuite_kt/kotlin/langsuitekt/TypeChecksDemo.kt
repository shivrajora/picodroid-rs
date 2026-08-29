// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * `is` / `!is` / `as` / `as?` and smart casts on boxes, strings, arrays, builtin collections, user
 * types.
 */
object TypeChecksDemo {
    private const val TAG = "TypeChecksKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Any?): Any? = v

    private interface Animal {
        fun sound(): String
    }

    private class Dog : Animal {
        override fun sound() = "woof"
    }

    private class Cat : Animal {
        override fun sound() = "meow"

        fun purr() = "purr"
    }

    private enum class Color {
        RED
    }

    private fun describe(x: Any?): String =
        when (x) {
            null -> "null"
            is String -> "str:${x.length}"
            is Int -> "int:${x + 1}"
            is Long -> "long"
            is Float -> "float"
            is Double -> "double"
            is Boolean -> "bool:${!x}"
            is Char -> "char"
            is IntArray -> "ints:${x.size}"
            is Array<*> -> "arr:${x.size}"
            is List<*> -> "list:${x.size}"
            is Map<*, *> -> "map"
            is Set<*> -> "set"
            is Pair<*, *> -> "pair"
            is Animal -> x.sound()
            is Enum<*> -> "enum:${x.name}"
            else -> "other"
        }

    fun run() {
        Log.i(TAG, "=== Type Check Tests ===")

        check("is String / smart cast", describe("abc") == "str:3")
        check("is Int (boxed)", describe(41) == "int:42")
        check(
            "is Long / Float / Double / Boolean / Char",
            describe(1L) == "long" &&
                describe(1f) == "float" &&
                describe(1.0) == "double" &&
                describe(true) == "bool:false" &&
                describe('c') == "char",
        )
        check(
            "is IntArray / Array",
            describe(intArrayOf(1, 2)) == "ints:2" && describe(arrayOf("a")) == "arr:1",
        )
        check(
            "is List / Map / Set",
            describe(listOf(1)) == "list:1" &&
                describe(mapOf(1 to 2)) == "map" &&
                describe(setOf(1)) == "set",
        )
        check(
            "is Pair / user interface / enum",
            describe(1 to 2) == "pair" &&
                describe(Dog()) == "woof" &&
                describe(Color.RED) == "enum:RED",
        )
        check("null branch / else", describe(null) == "null" && describe(Any()) == "other")
        val a: Any = opaque("text")!!
        check("as String", (a as String).length == 4)
        check(
            "as? success / failure",
            (a as? String)?.length == 4 &&
                (a as? Int) == null &&
                (opaque(3) as? Number)?.toInt() == 3,
        )
        check(
            "as failure throws ClassCastException",
            try {
                opaque(a as Int)
                false
            } catch (e: ClassCastException) {
                true
            },
        )
        check("null as? T", (opaque(null) as? String) == null)
        check(
            "null as T throws NPE",
            try {
                opaque(opaque(null) as String)
                false
            } catch (e: NullPointerException) {
                true
            },
        )
        val n: Any = opaque(5)!!
        check(
            "is Number / Comparable / CharSequence",
            n is Number &&
                (opaque("s") as Any) is CharSequence &&
                n is Comparable<*> &&
                (opaque("s") as Any) is Comparable<*>,
        )
        check(
            "same-type accessors after Number cast",
            (n as Number).toInt() == 5 &&
                (opaque(2.5f) as Number).toFloat() == 2.5f &&
                (opaque(7L) as Number).toLong() == 7L,
        )
        check("!is", (opaque(n) as Any) !is String && opaque(null) !is String)
        check(
            "smart cast in if",
            run {
                val v: Any = opaque(listOf(1, 2, 3))!!
                if (v is List<*>) v.size == 3 else false
            },
        )
        check(
            "smart cast && chain",
            run {
                val v: Any? = opaque("kt")
                v is String && v.length == 2
            },
        )
        check(
            "smart cast in while",
            run {
                var v: Any? = opaque(3)
                var loops = 0
                while (v is Int && v > 0) {
                    v = opaque(v - 1)
                    loops++
                }
                loops == 3
            },
        )
        val cat: Animal = Cat()
        check("cast to subtype", (cat as Cat).purr() == "purr" && (opaque(cat) as? Dog) == null)
        check(
            "interface / Any checks",
            (opaque(cat) as Any) is Animal && (opaque(cat) as Any) !is Dog,
        )
        check(
            "is Iterable / Collection",
            (opaque(listOf(1)) as Any) is Iterable<*> &&
                (opaque(setOf(1)) as Any) is Collection<*> &&
                (opaque(mapOf(1 to 1)) as Any) !is Collection<*>,
        )
        check(
            "array checks",
            (opaque(intArrayOf()) as Any) is IntArray &&
                (opaque(intArrayOf()) as Any) !is Array<*> &&
                (opaque(arrayOf("x")) as Any) is Array<*> &&
                (opaque(floatArrayOf(1f)) as Any) is FloatArray,
        )
        check(
            "boxed identity of type",
            (opaque(1) as Any) is Int &&
                (opaque(1L) as Any) !is Int &&
                (opaque(1.0f) as Any) !is Double,
        )
        check("Unit is Any", (opaque(Unit) as Any) is Unit)
        check("String is not Int", (opaque("1") as Any) !is Int)
        check(
            "when with is + else expression",
            when (val v = opaque(2.5)) {
                is Float -> "f"
                is Double -> "d${v.toInt()}"
                else -> "?"
            } == "d2",
        )
        check(
            "nullable Int smart-cast",
            run {
                val v: Int? = opaque(4) as Int?
                if (v != null) v + 1 == 5 else false
            },
        )
        check(
            "Any? equality across types",
            opaque(1) != opaque("1") && opaque(1) == opaque(1) && opaque(null) == opaque(null),
        )
        check(
            "checkcast List then iterate",
            run {
                val v: Any = opaque(listOf("a", "b"))!!
                var s = ""
                for (e in v as List<*>) s += e
                s == "ab"
            },
        )
        check("Enum is Comparable", (opaque(Color.RED) as Any) is Comparable<*>)
        check(
            "is on user class hierarchy",
            (opaque(Dog()) as Any) is Animal && (opaque(Any()) as Any) !is Animal,
        )
        check(
            "as? on Map",
            (opaque(mapOf("k" to 1)) as? Map<*, *>)?.size == 1 &&
                (opaque(listOf(1)) as? Map<*, *>) == null,
        )
        check("Char box checks", (opaque('c') as Any) is Char && (opaque('c') as Any) !is String)
        check(
            "Boolean box checks",
            (opaque(true) as Any) is Boolean && (opaque(true) as Any) !is Int,
        )
        check("String? to String via !!", (opaque("ok") as String?)!!.length == 2)
        check(
            "safe cast chain",
            ((opaque(listOf("z")) as? List<*>)?.firstOrNull() as? String)?.length == 1,
        )
        check(
            "is Pair with destructuring",
            run {
                val p: Any = opaque(1 to "x")!!
                if (p is Pair<*, *>) {
                    val (l, r) = p
                    l == 1 && r == "x"
                } else false
            },
        )

        Check.done(TAG)
    }
}
