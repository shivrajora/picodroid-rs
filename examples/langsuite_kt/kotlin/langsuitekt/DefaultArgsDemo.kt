// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Default arguments: the `$default` bridges (bitmask + `DefaultConstructorMarker` for
 * constructors), named arguments, defaults on members, open classes, interfaces and generics.
 */
object DefaultArgsDemo {
    private const val TAG = "DefaultArgsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun greet(name: String, greeting: String = "hi", punct: Char = '!') =
        "$greeting $name$punct"

    private fun area(w: Int, h: Int = w) = w * h

    private fun sumAll(a: Int = 1, b: Int = 2, c: Int = 3) = a + b + c

    private fun withLambda(x: Int, f: (Int) -> Int = { it * 2 }) = f(x)

    private fun nullableDefault(s: String? = null) = s ?: "none"

    private fun varargDefault(sep: String = ",", vararg parts: String) = parts.joinToString(sep)

    private fun defaultFromExpr(now: Int, later: Int = now + 10) = later - now

    private fun <T> firstOr(list: List<T>, dflt: T? = null): T? =
        if (list.isEmpty()) dflt else list[0]

    private fun floatDefaults(a: Float = 1.5f, b: Double = 2.5, c: Long = 3L, d: Boolean = true) =
        if (d) a + b.toFloat() + c else 0f

    private class Config(
        val host: String = "localhost",
        val port: Int = 8080,
        val secure: Boolean = false,
    ) {
        override fun toString() = "$host:$port/$secure"
    }

    private open class Base {
        open fun describe(prefix: String = "base") = "$prefix!"
    }

    private class Derived : Base() {
        override fun describe(prefix: String) = "derived-$prefix"
    }

    private interface Shape {
        fun scaled(factor: Int = 2): Int
    }

    private class Sq(val s: Int) : Shape {
        override fun scaled(factor: Int) = s * factor
    }

    private class Counter {
        private var n = 0

        fun bump(by: Int = 1): Int {
            n += by
            return n
        }
    }

    private class Secondary(val x: Int) {
        constructor(s: String, extra: Int = 1) : this(s.length + extra)
    }

    fun run() {
        Log.i(TAG, "=== Default Argument Tests ===")

        check("all defaults", greet("bo") == "hi bo!")
        check("positional override", greet("bo", "yo") == "yo bo!")
        check("named override", greet("bo", punct = '?') == "hi bo?")
        check("named all", greet(punct = '.', greeting = "hey", name = "x") == "hey x.")
        check("default depends on param", area(3) == 9 && area(3, 2) == 6)
        check(
            "middle default",
            sumAll(b = 10) == 14 && sumAll(c = 0) == 3 && sumAll() == 6 && sumAll(5, 5, 5) == 15,
        )
        check("lambda default", withLambda(4) == 8 && withLambda(4) { it + 1 } == 5)
        check("nullable default", nullableDefault() == "none" && nullableDefault("s") == "s")
        check(
            "constructor defaults",
            Config().toString() == "localhost:8080/false" &&
                Config(port = 1).toString() == "localhost:1/false" &&
                Config("h", 2, true).toString() == "h:2/true",
        )
        check(
            "open class default inherited",
            Base().describe() == "base!" &&
                Derived().describe() == "derived-base" &&
                Derived().describe("p") == "derived-p",
        )
        check("interface default arg", Sq(3).scaled() == 6 && Sq(3).scaled(3) == 9)
        val c = Counter()
        check("member default", c.bump() == 1 && c.bump(5) == 6)
        check(
            "vararg with default",
            varargDefault(",", "a", "b") == "a,b" &&
                varargDefault("-", "x", "y") == "x-y" &&
                varargDefault() == "",
        )
        check("default from expression", defaultFromExpr(5) == 10 && defaultFromExpr(5, 6) == 1)
        check(
            "generic default",
            firstOr(emptyList<String>()) == null &&
                firstOr(emptyList(), "d") == "d" &&
                firstOr(listOf("z")) == "z",
        )
        val poly: Base = Derived()
        check("default through base ref", poly.describe() == "derived-base")
        val sh: Shape = Sq(2)
        check("default through interface ref", sh.scaled() == 4)
        check(
            "float / double / long / boolean defaults",
            floatDefaults() == 7f && floatDefaults(d = false) == 0f && floatDefaults(b = 0.5) == 5f,
        )
        check(
            "secondary constructor default",
            Secondary("abc").x == 4 && Secondary("abc", 2).x == 5 && Secondary(9).x == 9,
        )
        check("default in lambda param call", listOf(1, 2).map { greet("n$it") }.last() == "hi n2!")
        check("trailing lambda with default first", withLambda(x = 3) == 6)

        Check.done(TAG)
    }
}
