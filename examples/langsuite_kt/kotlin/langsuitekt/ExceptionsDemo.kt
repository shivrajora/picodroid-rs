// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Exceptions: `require`/`check`/`error`/`TODO`, custom exception classes over builtin bases, catch
 * ordering and base-type catches, `finally`, `try` as an expression, propagation through a lambda,
 * rethrow.
 */
object ExceptionsDemo {
    private const val TAG = "ExceptionsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    class MyEx(msg: String) : Exception(msg)

    class MyRt(msg: String, val code: Int) : RuntimeException(msg)

    private fun risky(n: Int): Int {
        if (n < 0) throw MyRt("neg", n)
        return n
    }

    private fun opaque(v: Boolean): Boolean = v

    /**
     * A non-inline `Nothing` function: kotlinc guards the call with a
     * `KotlinNothingValueException`.
     */
    private fun fail(code: Int): Nothing = throw MyRt("fail", code)

    private fun guarded(n: Int): Int = if (n > 0) n else fail(n)

    fun run() {
        Log.i(TAG, "=== Exception Tests ===")

        var msg = ""
        try {
            require(opaque(false)) { "bad ${1 + 1}" }
        } catch (e: IllegalArgumentException) {
            msg = e.message ?: ""
        }
        check("require", msg == "bad 2")
        msg = ""
        try {
            check(opaque(false)) { "chk" }
        } catch (e: IllegalStateException) {
            msg = e.message ?: ""
        }
        check("check", msg == "chk")
        msg = ""
        try {
            error("boom")
        } catch (e: IllegalStateException) {
            msg = e.message ?: ""
        }
        check("error", msg == "boom")
        msg = ""
        try {
            TODO("later")
        } catch (e: NotImplementedError) {
            msg = e.message ?: ""
        }
        check("TODO(reason)", msg == "An operation is not implemented: later")
        msg = ""
        try {
            TODO()
        } catch (e: Error) {
            msg = e.message ?: ""
        }
        check("TODO() caught as Error", msg == "An operation is not implemented.")

        var code = 0
        try {
            risky(-7)
        } catch (e: MyRt) {
            code = e.code
        }
        check("custom RuntimeException field", code == -7)
        msg = ""
        try {
            throw MyEx("checked-ish")
        } catch (e: Exception) {
            msg = e.message ?: ""
        }
        check("custom Exception via base catch", msg == "checked-ish")

        val trace = StringBuilder()
        try {
            try {
                trace.append("a")
                risky(-1)
                trace.append("x")
            } finally {
                trace.append("f")
            }
        } catch (e: MyRt) {
            trace.append("c")
        }
        check("finally then outer catch", trace.toString() == "afc")

        val trace2 = StringBuilder()
        try {
            trace2.append("a")
        } finally {
            trace2.append("f")
        }
        check("finally without throw", trace2.toString() == "af")

        val v =
            try {
                risky(-2)
            } catch (e: MyRt) {
                7
            }
        check("try as expression", v == 7)
        val w =
            try {
                risky(5)
            } catch (e: MyRt) {
                7
            }
        check("try as expression no throw", w == 5)

        val f: () -> Int = { risky(-3) }
        var viaLambda = 0
        try {
            f()
        } catch (e: MyRt) {
            viaLambda = e.code
        }
        check("throw through Function0", viaLambda == -3)

        var order = ""
        try {
            risky(-4)
        } catch (e: MyRt) {
            order = "specific"
        } catch (e: RuntimeException) {
            order = "general"
        }
        check("catch ordering", order == "specific")

        var rethrown = false
        try {
            try {
                risky(-5)
            } catch (e: MyRt) {
                throw e
            }
        } catch (e: RuntimeException) {
            rethrown = e is MyRt
        }
        check("rethrow", rethrown)

        var anyCaught = false
        try {
            throw MyEx("t")
        } catch (t: Throwable) {
            anyCaught = t.message == "t"
        }
        check("catch Throwable", anyCaught)

        var nothingCode = 0
        try {
            nothingCode = guarded(-9)
        } catch (e: MyRt) {
            nothingCode = e.code
        }
        check("Nothing-returning function", guarded(4) == 4 && nothingCode == -9)

        Check.done(TAG)
    }
}
