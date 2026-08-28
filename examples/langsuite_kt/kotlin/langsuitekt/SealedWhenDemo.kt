// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Sealed hierarchies and every `when` shape: sealed `is`/object branches, subject-less, ranges,
 * strings (few and many branches), enums, `Any`.
 */
object SealedWhenDemo {
    private const val TAG = "SealedWhenKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    sealed class Sample {
        data class Ok(val v: Int) : Sample()

        object Missing : Sample()

        class Err(val msg: String) : Sample()
    }

    private fun render(s: Sample): String =
        when (s) {
            is Sample.Ok -> "ok ${s.v}"
            Sample.Missing -> "missing"
            is Sample.Err -> "err ${s.msg}"
        }

    private fun classify(n: Int): String =
        when {
            n < 0 -> "neg"
            n == 0 -> "zero"
            n in 1..9 -> "small"
            else -> "big"
        }

    private fun few(s: String): Int =
        when (s) {
            "a" -> 1
            "b" -> 2
            else -> 0
        }

    private fun many(s: String): Int =
        when (s) {
            "one",
            "uno" -> 1
            "two" -> 2
            "three" -> 3
            "four" -> 4
            "five" -> 5
            "six" -> 6
            "seven" -> 7
            else -> 0
        }

    private fun describe(x: Any?): String =
        when (x) {
            null -> "null"
            is Int -> "int ${x + 1}"
            is String -> "str ${x.length}"
            is Sample -> "sample"
            else -> "other"
        }

    private fun opaque(v: Any?): Any? = v

    fun run() {
        Log.i(TAG, "=== Sealed / When Tests ===")

        check("sealed data branch", render(Sample.Ok(3)) == "ok 3")
        check("sealed object branch", render(Sample.Missing) == "missing")
        check("sealed class branch", render(Sample.Err("e")) == "err e")
        val samples = arrayOf<Sample>(Sample.Ok(1), Sample.Missing, Sample.Err("x"))
        var okCount = 0
        for (s in samples) if (s is Sample.Ok) okCount += s.v
        check("sealed in loop", okCount == 1)
        check("sealed object identity", Sample.Missing === Sample.Missing)
        check("sealed data equals", Sample.Ok(2) == Sample.Ok(2))

        check(
            "subjectless when",
            classify(-1) == "neg" &&
                classify(0) == "zero" &&
                classify(5) == "small" &&
                classify(50) == "big",
        )
        check("when on String (few)", few("a") == 1 && few("b") == 2 && few("q") == 0)
        check(
            "when on String (many)",
            many("one") == 1 && many("uno") == 1 && many("seven") == 7 && many("zzz") == 0,
        )
        check(
            "when on Any",
            describe(opaque(null)) == "null" &&
                describe(opaque(4)) == "int 5" &&
                describe(opaque("ab")) == "str 2",
        )
        check(
            "when on Any sealed/other",
            describe(opaque(Sample.Missing)) == "sample" && describe(opaque(1.5f)) == "other",
        )

        val e = opaque(NoWhenBranchMatchedException())
        check("NoWhenBranchMatchedException is RuntimeException", e is RuntimeException)
        var caught = false
        try {
            throw NoWhenBranchMatchedException()
        } catch (ex: RuntimeException) {
            caught = true
        }
        check("NoWhenBranchMatchedException caught", caught)

        Check.done(TAG)
    }
}
