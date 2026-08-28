// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Interface default methods under `-Xjvm-default=all`: no override, class override, sub-interface
 * override in either `implements` order, `super<I>.f()` in a diamond, defaults through abstract
 * classes, default properties, defaults calling abstract members, `object` implementors.
 */
object InterfaceDefaultDemo {
    private const val TAG = "InterfaceDefaultKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    interface Describable {
        fun describe(): String = "describable"

        val tag: String
            get() = "d"
    }

    interface Tagged {
        fun tag(): String

        fun describe(): String = "tagged:" + tag()
    }

    class Both : Describable, Tagged {
        override fun tag(): String = "b"

        override fun describe(): String =
            super<Describable>.describe() + "+" + super<Tagged>.describe()
    }

    class OnlyDefault : Describable

    class Overrider : Describable {
        override fun describe(): String = "overridden"

        override val tag: String
            get() = "o"
    }

    interface Sub : Describable {
        override fun describe(): String = "sub"
    }

    class ViaSub : Sub

    class ViaBoth : Describable, Sub

    abstract class Base : Describable

    class Leaf : Base()

    interface Counter {
        fun next(): Int

        fun skip(n: Int): Int {
            var last = 0
            repeat(n) { last = next() }
            return last + bonus()
        }

        fun bonus(): Int = 100
    }

    class Up : Counter {
        var c = 0

        override fun next(): Int {
            c++
            return c
        }
    }

    object Singleton : Describable

    fun run() {
        Log.i(TAG, "=== Interface Default Tests ===")

        check("default without override", OnlyDefault().describe() == "describable")
        val d: Describable = OnlyDefault()
        check("default via interface ref", d.describe() == "describable")
        check("default property getter", d.tag == "d")
        check("class override", Overrider().describe() == "overridden" && Overrider().tag == "o")
        check("sub-interface default", ViaSub().describe() == "sub")
        check("most specific wins over order", ViaBoth().describe() == "sub")
        check("super<I>.f() diamond", Both().describe() == "describable+tagged:b")
        val t: Tagged = Both()
        check("diamond via other iface", t.describe() == "describable+tagged:b")
        check("through abstract class", Leaf().describe() == "describable")
        check("default calls abstract on this", Up().skip(3) == 103)
        check("object implementor", Singleton.describe() == "describable" && Singleton.tag == "d")
        check("instanceof through default iface", (Singleton as Any) is Describable)

        Check.done(TAG)
    }
}
