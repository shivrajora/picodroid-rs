// SPDX-License-Identifier: GPL-3.0-only
package langsuitektstdlib

import picodroid.util.Log

/**
 * String templates, raw strings, `when(string)`, the `StringsKt` extension surface, `CharsKt`,
 * parsing, formatting, and the char/string HOFs.
 */
object StringsDemo {
    private const val TAG = "StringsKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Any?): Any? = v

    private class Pt(val x: Int, val y: Int) {
        override fun toString() = "($x,$y)"
    }

    fun run() {
        Log.i(TAG, "=== Strings Tests ===")

        val name = "pico"
        val n = 42
        val f = 1.5f
        check("template simple", "hi $name" == "hi pico" && "n=$n" == "n=42")
        check("template expr", "${n + 1} ${name.length}" == "43 4")
        check("template float / bool / char", "$f ${n > 1} ${'c'}" == "1.5 true c")
        check("template null", "${opaque(null)}" == "null")
        check("template object", "${Pt(1, 2)}" == "(1,2)" && "" + Pt(3, 4) == "(3,4)")
        check("template long / double", "${7L} ${2.5}" == "7 2.5")
        check("template nested", "a${"b${1 + 1}"}c" == "ab2c")
        val raw =
            """
            line1
              line2
            """
                .trimIndent()
        check("raw string trimIndent", raw == "line1\n  line2")
        check("escapes", "a\tb\n".length == 4 && "\$x" == "$" + "x" && "\"q\"".length == 3)
        check(
            "length / get / indexing",
            name.length == 4 && name[0] == 'p' && name[name.lastIndex] == 'o',
        )
        check("substring", name.substring(1) == "ico" && name.substring(1, 3) == "ic")
        check("plus / concat", name + "!" == "pico!" && "a".plus("b") == "ab")
        check(
            "equals / compareTo",
            name == "pico" && name != "Pico" && "a" < "b" && "b".compareTo("a") > 0,
        )
        check(
            "equals ignoreCase",
            name.equals("PICO", ignoreCase = true) && !name.equals("PICO", ignoreCase = false),
        )
        check("uppercase / lowercase", name.uppercase() == "PICO" && "MiX".lowercase() == "mix")
        check(
            "startsWith / endsWith",
            name.startsWith("pi") && name.endsWith("co") && !name.startsWith("x"),
        )
        check("startsWith ignoreCase", name.startsWith("PI", ignoreCase = true))
        check(
            "contains",
            "ic" in name && name.contains("co") && !name.contains("z") && name.contains('i'),
        )
        check("contains ignoreCase", name.contains("IC", ignoreCase = true))
        check(
            "indexOf / lastIndexOf",
            "banana".indexOf("an") == 1 &&
                "banana".lastIndexOf("an") == 3 &&
                "banana".indexOf('n') == 2 &&
                "banana".indexOf("x") == -1,
        )
        check("indexOf from", "banana".indexOf("an", 2) == 3 && "banana".indexOf('a', 1) == 1)
        check(
            "trim family",
            "  x y ".trim() == "x y" &&
                "  x ".trimStart() == "x " &&
                "  x ".trimEnd() == "  x" &&
                "\t\n x".trim() == "x",
        )
        check(
            "isBlank / isNotBlank / isEmpty / isNotEmpty",
            "  ".isBlank() &&
                "a".isNotBlank() &&
                "".isEmpty() &&
                "a".isNotEmpty() &&
                !"".isNotBlank(),
        )
        check(
            "isNullOrEmpty / isNullOrBlank / orEmpty",
            (opaque(null) as String?).isNullOrEmpty() &&
                " ".isNullOrBlank() &&
                (opaque(null) as String?).orEmpty() == "",
        )
        check(
            "ifEmpty / ifBlank",
            "".ifEmpty { "dflt" } == "dflt" &&
                " ".ifBlank { "b" } == "b" &&
                "x".ifEmpty { "d" } == "x",
        )
        check(
            "padStart / padEnd",
            "7".padStart(3, '0') == "007" &&
                "ab".padEnd(4) == "ab  " &&
                "long".padStart(2) == "long",
        )
        check("repeat", "ab".repeat(3) == "ababab" && "x".repeat(0) == "")
        check("reversed", name.reversed() == "ocip")
        check("replace", "a-b-c".replace("-", "+") == "a+b+c" && "aXa".replace('a', 'o') == "oXo")
        check("replace ignoreCase", "aAa".replace("a", "x", ignoreCase = true) == "xxx")
        check("split", "a,b,c".split(",").size == 3 && "a,b,c".split(",")[1] == "b")
        check("split multi delim", "a, b; c".split(", ", "; ").joinToString("|") == "a|b|c")
        check("split empty parts", "a,,b".split(",").size == 3 && ",a,".split(",").size == 3)
        check("split limit", "a,b,c".split(",", limit = 2)[1] == "b,c")
        check("split char", "1:2:3".split(':').map { it.toInt() }.sum() == 6)
        check("split ignoreCase", "aXbxc".split("x", ignoreCase = true).size == 3)
        check("split no match", "abc".split(",").size == 1 && "abc".split(",")[0] == "abc")
        check(
            "substringBefore / After",
            "k=v=w".substringBefore("=") == "k" &&
                "k=v=w".substringAfter("=") == "v=w" &&
                "k=v=w".substringAfterLast("=") == "w" &&
                "k=v=w".substringBeforeLast("=") == "k=v",
        )
        check(
            "substringBefore char / missing",
            "k=v".substringBefore('=') == "k" &&
                "kv".substringBefore("=") == "kv" &&
                "kv".substringAfter("=", "none") == "none",
        )
        check(
            "removePrefix / removeSuffix / removeSurrounding",
            "prefix_x".removePrefix("prefix_") == "x" &&
                "x.kt".removeSuffix(".kt") == "x" &&
                "[x]".removeSurrounding("[", "]") == "x" &&
                "y".removePrefix("z") == "y",
        )
        check(
            "take / drop / takeLast / dropLast",
            name.take(2) == "pi" &&
                name.drop(2) == "co" &&
                name.takeLast(1) == "o" &&
                name.dropLast(1) == "pic" &&
                name.take(10) == "pico",
        )
        check(
            "first / last / firstOrNull",
            name.first() == 'p' &&
                name.last() == 'o' &&
                "".firstOrNull() == null &&
                name.firstOrNull() == 'p',
        )
        check(
            "toInt / toIntOrNull",
            "42".toInt() == 42 &&
                "-7".toIntOrNull() == -7 &&
                "4x".toIntOrNull() == null &&
                "".toIntOrNull() == null &&
                "+3".toIntOrNull() == 3,
        )
        check(
            "toIntOrNull overflow",
            "99999999999".toIntOrNull() == null &&
                "2147483647".toIntOrNull() == Int.MAX_VALUE &&
                "-2147483648".toIntOrNull() == Int.MIN_VALUE,
        )
        check(
            "toLong / toLongOrNull",
            "9000000000".toLong() == 9000000000L &&
                "12".toLongOrNull() == 12L &&
                "z".toLongOrNull() == null,
        )
        check(
            "toFloat / toFloatOrNull",
            "1.5".toFloat() == 1.5f &&
                "2.25".toFloatOrNull() == 2.25f &&
                "abc".toFloatOrNull() == null,
        )
        check(
            "toDouble / toDoubleOrNull",
            "0.5".toDouble() == 0.5 && "2.5".toDoubleOrNull() == 2.5 && "".toDoubleOrNull() == null,
        )
        check("toBoolean", "true".toBoolean() && !"nope".toBoolean())
        check("format", "%d-%s".format(7, "x") == "7-x" && String.format("%.2f", 1.5f) == "1.50")
        check(
            "when(string)",
            when (opaque("b") as String) {
                "a" -> 1
                "b" -> 2
                else -> 0
            } == 2,
        )
        check(
            "when(string) else",
            when (name) {
                "x",
                "y" -> 1
                else -> 9
            } == 9,
        )
        check(
            "chars: isDigit / isLetter / isWhitespace",
            '7'.isDigit() &&
                'a'.isLetter() &&
                ' '.isWhitespace() &&
                !'a'.isDigit() &&
                !'x'.isWhitespace() &&
                '\t'.isWhitespace(),
        )
        check(
            "chars: digitToInt / code / arithmetic",
            '7'.digitToInt() == 7 &&
                'a'.code == 97 &&
                ('a' + 1) == 'b' &&
                ('c' - 'a') == 2 &&
                98.toChar() == 'b',
        )
        check(
            "chars: uppercaseChar / lowercaseChar",
            'a'.uppercaseChar() == 'A' && 'Q'.lowercaseChar() == 'q' && 'a'.uppercase() == "A",
        )
        check(
            "count / filter / map on string",
            name.count { it == 'p' } == 1 &&
                name.filter { it != 'i' } == "pco" &&
                name.map { it.uppercaseChar() }.joinToString("") == "PICO",
        )
        check(
            "forEach / any / all / none",
            run {
                var c = 0
                name.forEach { c += it.code }
                c == 427
            } && name.any { it == 'o' } && name.all { it.isLetter() } && name.none { it.isDigit() },
        )
        check(
            "toCharArray / toList / toSet",
            name.toCharArray()[1] == 'i' && name.toList().size == 4 && "aab".toSet().size == 2,
        )
        check(
            "indices / forEachIndexed",
            name.indices.last == 3 &&
                run {
                    var t = 0
                    name.forEachIndexed { i, c -> t += i * c.code }
                    t == 105 + 2 * 99 + 3 * 111
                },
        )
        check(
            "buildString",
            buildString {
                append("a")
                append(1)
                append('c')
                append(2.5f)
            } == "a1c2.5",
        )
        check(
            "StringBuilder ops",
            StringBuilder().append("x").append(1).append(true).toString() == "x1true" &&
                StringBuilder("ab").length == 2,
        )
        check(
            "hashCode consistency",
            "abc".hashCode() == 96354 && "abc".hashCode() == ("ab" + "c").hashCode(),
        )
        check("lines via split", "a\nb\nc".split("\n").size == 3)
        check(
            "String.toIntOrNull in map",
            listOf("1", "x", "3").mapNotNull { it.toIntOrNull() }.sum() == 4,
        )
        check(
            "sortedBy lowercase",
            listOf("b", "A", "c").sortedBy { it.lowercase() }.joinToString("") == "Abc",
        )
        check(
            "String? safe ops",
            (opaque(null) as String?)?.length == null && (opaque("ab") as String?)?.length == 2,
        )
        check("CharSequence length", (opaque("abc") as CharSequence).length == 3)
        check(
            "substring with range",
            name.substring(1 until 3) == "ic" && name.substring(1..2) == "ic",
        )
        check(
            "lastIndex / getOrNull",
            name.lastIndex == 3 && name.getOrNull(9) == null && name.getOrNull(0) == 'p',
        )
        check(
            "String compare in when",
            when {
                name.length > 3 -> "long"
                else -> "short"
            } == "long",
        )
        check("trimIndent with blank lines", "\n  a\n\n  b\n".trimIndent() == "a\n\nb")
        check("trimMargin", "|a\n  |b".trimMargin() == "a\nb")
        check("startsWith char / endsWith char", name.startsWith('p') && name.endsWith('o'))
        check("compareTo ignoreCase", "abc".compareTo("ABC", ignoreCase = true) == 0)
        check("indexOfFirst on string", name.indexOfFirst { it == 'c' } == 2)
        check(
            "String.iterator",
            run {
                var c = 0
                for (ch in name) c++
                c == 4
            },
        )
        check("lowercase in template", "${name.uppercase()}!" == "PICO!")
        check(
            "drop while / take while",
            "  x".dropWhile { it == ' ' } == "x" && "ab1".takeWhile { it.isLetter() } == "ab",
        )
        check("zip strings", "ab".zip("xy").size == 2 && "ab".zip("xy")[1] == ('b' to 'y'))
        check("String.sum-like fold", name.fold(0) { acc, c -> acc + c.code } == 427)
        check(
            "CharArray joinToString / concatToString",
            charArrayOf('o', 'k').joinToString("") == "ok" &&
                charArrayOf('o', 'k').concatToString() == "ok",
        )
        check(
            "String equality across builds",
            ("pi" + "co") == name && ("pi" + "co").hashCode() == name.hashCode(),
        )
        check("length of template with unicode-free escapes", "\\".length == 1)
        check("isDigit on all chars", "123".all { it.isDigit() } && !"12a".all { it.isDigit() })
        check("uppercase(char) via map", "ab".map { it.uppercaseChar() }.joinToString("") == "AB")
        check("toCharArray sorted", name.toCharArray().sorted().joinToString("") == "ciop")

        Check.done(TAG)
    }
}
