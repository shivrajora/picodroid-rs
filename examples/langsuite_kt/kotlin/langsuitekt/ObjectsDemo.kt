// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * `object` singletons, companion objects (`const`/`val`/`@JvmField`/`@JvmStatic`), init order,
 * inner and nested classes.
 */
object ObjectsDemo {
    private const val TAG = "ObjectsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    object Registry {
        const val MAX = 3
        val created = ArrayList<String>()
        @JvmField var version = 3
        var inits = 0

        init {
            inits++
        }

        @JvmStatic
        fun register(n: String): Boolean {
            if (created.size < MAX) {
                created.add(n)
                return true
            }
            return false
        }

        override fun toString(): String = "Registry(${created.size})"
    }

    class Config(val limit: Int, val name: String) {
        companion object {
            const val KEY = "limit"
            val DEFAULT = Config(60, "default")
            @JvmField val NAMES = arrayOf("a", "b")

            @JvmStatic fun of(limit: Int): Config = Config(limit, "x")

            fun named(n: String): Config = Config(0, n)
        }
    }

    class Outer(val base: Int) {
        inner class Inner(val d: Int) {
            fun total(): Int = base + d
        }

        class Nested(val v: Int) {
            fun twice(): Int = v * 2
        }
    }

    object Counter {
        var n = 0

        fun next(): Int {
            n++
            return n
        }
    }

    class Ordered {
        val trace = StringBuilder()
        val a = record("a")

        init {
            record("init")
        }

        val b = record("b")

        private fun record(s: String): String {
            trace.append(s)
            return s
        }
    }

    fun run() {
        Log.i(TAG, "=== Objects Tests ===")

        check("register 1", Registry.register("a"))
        check("register 2", Registry.register("b"))
        check("register 3", Registry.register("c"))
        check("register over MAX", !Registry.register("d"))
        check("object state", Registry.created.size == 3)
        check("const val", Registry.MAX == 3)
        check("@JvmField var", Registry.version == 3)
        Registry.version = 4
        check("@JvmField write", Registry.version == 4)
        check("init block once", Registry.inits == 1)
        val r1 = Registry
        val r2 = Registry
        check("object identity", r1 === r2)
        check("object toString override via template", "$r1" == "Registry(3)")

        check("companion val", Config.DEFAULT.limit == 60 && Config.DEFAULT.name == "default")
        check("companion const", Config.KEY == "limit")
        check("companion @JvmField array", Config.NAMES.size == 2 && Config.NAMES[1] == "b")
        check("companion @JvmStatic", Config.of(5).limit == 5)
        check("companion fun", Config.named("z").name == "z")

        check("inner class captures outer", Outer(10).Inner(5).total() == 15)
        check("nested class", Outer.Nested(4).twice() == 8)

        Counter.next()
        check("object var", Counter.next() == 2)

        val o = Ordered()
        check("property/init order", o.trace.toString() == "ainitb" && o.a == "a" && o.b == "b")

        val anon =
            object : Runnable {
                var hits = 0

                override fun run() {
                    hits++
                }
            }
        anon.run()
        anon.run()
        check("object expression", anon.hits == 2)

        Check.done(TAG)
    }
}
