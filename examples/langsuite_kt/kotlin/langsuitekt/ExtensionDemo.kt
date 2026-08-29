// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

private fun String.shout() = uppercase() + "!"

private fun Int.squared() = this * this

private val String.firstChar: Char
    get() = this[0]

private fun String?.orDash() = this ?: "-"

private fun Int?.orZero() = this ?: 0

private fun <T> List<T>.secondOrNull(): T? = if (size >= 2) this[1] else null

private fun List<Int>.total() = sum()

private infix fun Int.pow2(e: Int): Int {
    var r = 1
    repeat(e) { r *= this }
    return r
}

private operator fun String.times(n: Int) = repeat(n)

private fun String.applyTwice(f: String.() -> String) = f().f()

private fun Int.isEven() = this % 2 == 0

private fun <T : Comparable<T>> List<T>.maxOrFirst(): T =
    if (isEmpty()) throw NoSuchElementException() else max()

private class Vec(val x: Int, val y: Int) {
    operator fun plus(o: Vec) = Vec(x + o.x, y + o.y)

    operator fun minus(o: Vec) = Vec(x - o.x, y - o.y)

    operator fun times(k: Int) = Vec(x * k, y * k)

    operator fun unaryMinus() = Vec(-x, -y)

    operator fun get(i: Int) = if (i == 0) x else y

    operator fun contains(v: Int) = v == x || v == y

    operator fun compareTo(o: Vec) = (x * x + y * y) - (o.x * o.x + o.y * o.y)

    operator fun invoke(k: Int) = x * k + y

    operator fun component1() = x

    operator fun component2() = y

    override fun equals(other: Any?) = other is Vec && other.x == x && other.y == y

    override fun hashCode() = x * 31 + y

    override fun toString() = "($x,$y)"
}

private operator fun Vec.not() = Vec(y, x)

private class Counter(var n: Int) {
    operator fun inc() = Counter(n + 1)

    operator fun plusAssign(k: Int) {
        n += k
    }
}

private class Grid {
    private val cells = IntArray(4)

    operator fun get(r: Int, c: Int) = cells[r * 2 + c]

    operator fun set(r: Int, c: Int, v: Int) {
        cells[r * 2 + c] = v
    }
}

private object Registry {
    fun String.reg() = "reg:$this"

    fun use() = "x".reg()
}

/** Extension functions/properties, infix, operator overloading, lambdas with receiver. */
object ExtensionDemo {
    private const val TAG = "ExtensionKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Any?): Any? = v

    fun run() {
        Log.i(TAG, "=== Extension / Operator Tests ===")

        check("String extension", "hi".shout() == "HI!")
        check("Int extension", 7.squared() == 49 && (-3).squared() == 9)
        check("extension property", "kotlin".firstChar == 'k')
        check("nullable receiver", (opaque(null) as String?).orDash() == "-" && "s".orDash() == "s")
        check(
            "generic list extension",
            listOf(1, 2).secondOrNull() == 2 && listOf("a").secondOrNull() == null,
        )
        check("List<Int> extension", listOf(1, 2, 3).total() == 6)
        check("infix", (2 pow2 10) == 1024 && 3.pow2(2) == 9)
        val a = Vec(1, 2)
        val b = Vec(3, 4)
        check(
            "plus / minus / times",
            (a + b) == Vec(4, 6) && (b - a) == Vec(2, 2) && (a * 3) == Vec(3, 6),
        )
        check("unaryMinus / not", -a == Vec(-1, -2) && !a == Vec(2, 1))
        check("get / contains", a[0] == 1 && a[1] == 2 && 2 in a && 5 !in a)
        check("compareTo", a < b && b > a && a <= Vec(2, 1) && !(a < Vec(2, 1)))
        check("invoke", a(10) == 12)
        val (x, y) = b
        check("componentN destructuring", x == 3 && y == 4)
        var c = Counter(0)
        c++
        check("inc", c.n == 1)
        c += 5
        check("plusAssign", c.n == 6)
        val g = Grid()
        g[1, 1] = 9
        check("indexed get/set", g[1, 1] == 9 && g[0, 0] == 0)
        check("extension in object", Registry.use() == "reg:x")
        check("lambda with receiver", "a".applyTwice { this + "b" } == "abb")
        check("extension in lambda", listOf(1, 2, 3).filter { it.isEven() }.size == 1)
        check(
            "extension via generic bound",
            listOf(3, 9, 1).maxOrFirst() == 9 && listOf("q").maxOrFirst() == "q",
        )
        check("extension in map", listOf("a", "b").map { it.shout() }.joinToString() == "A!, B!")
        check("member toString wins", Vec(1, 1).toString() == "(1,1)")
        check("string plus char", "ab" + 'c' == "abc")
        check("rangeTo operator", (1..3).count() == 3)
        check("in operator on collection", 2 in listOf(1, 2))
        check("equals operator", a == Vec(1, 2) && a != b && a.equals(Vec(1, 2)))
        check("hashCode override", a.hashCode() == 33 && Vec(1, 2).hashCode() == a.hashCode())
        check(
            "extension on nullable Int",
            (opaque(null) as Int?).orZero() == 0 && (opaque(5) as Int?).orZero() == 5,
        )
        check("extension in let", "x".let { s -> s.shout() } == "X!")
        check(
            "compound operators on numbers",
            run {
                var v = 10
                v += 5
                v -= 3
                v *= 2
                v /= 4
                v %= 5
                v == 1
            },
        )
        check("String times via repeat", "ab" * 2 == "abab")
        check("Vec template", "$a" == "(1,2)" && "${a + b}" == "(4,6)")
        check("operator chain", (a + b - a) == b && -(-a) == a)
        check("destructuring in lambda", listOf(a, b).map { (px, py) -> px + py }.sum() == 10)
        check(
            "extension on Any?",
            (opaque(null) as Any?).describeNull() == "nothing" && 1.describeNull() == "something",
        )

        Check.done(TAG)
    }

    private fun Any?.describeNull() = if (this == null) "nothing" else "something"
}
