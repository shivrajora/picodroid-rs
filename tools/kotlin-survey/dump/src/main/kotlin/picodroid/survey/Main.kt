// SPDX-License-Identifier: GPL-3.0-only
package picodroid.survey

import java.io.File
import kotlin.system.exitProcess

/**
 * CLI for the survey. All file I/O lives here; everything else is pure.
 *
 *   dump  --label L --classes DIR [--classes DIR ...] --out DIR [--external REGEX]
 *   strip --label L --classes DIR --out DIR
 */
fun main(args: Array<String>) {
    val cmd = args.firstOrNull()
    val opts = parseOptions(args.drop(1))
    when (cmd) {
        "dump" -> dumpCommand(opts)
        "strip" -> stripCommand(opts)
        else -> {
            System.err.println("usage: dump|strip --label L --classes DIR... --out DIR [--external REGEX]")
            exitProcess(2)
        }
    }
}

private class Options(val label: String, val classes: List<File>, val out: File, val external: Regex)

private fun parseOptions(args: List<String>): Options {
    var label: String? = null
    val classes = ArrayList<File>()
    var out: File? = null
    var external = "^(kotlin|kotlinx|java|javax)/"
    var i = 0
    while (i < args.size) {
        when (val a = args[i]) {
            "--label" -> label = args[++i]
            "--classes" -> classes += File(args[++i])
            "--out" -> out = File(args[++i])
            "--external" -> external = args[++i]
            else -> {
                System.err.println("unknown option: $a")
                exitProcess(2)
            }
        }
        i++
    }
    if (label == null || classes.isEmpty() || out == null) {
        System.err.println("--label, --classes and --out are required")
        exitProcess(2)
    }
    return Options(label, classes, out, Regex(external))
}

/** Every `.class` under [root], as (relative path with '/' separators, bytes), sorted by path. */
private fun classFiles(root: File): List<Pair<String, ByteArray>> {
    require(root.isDirectory) { "not a directory: $root" }
    return root.walkTopDown()
        .filter { it.isFile && it.name.endsWith(".class") }
        .map { it.relativeTo(root).invariantSeparatorsPath to it.readBytes() }
        .sortedBy { it.first }
        .toList()
}

private fun write(out: File, name: String, text: String) {
    out.mkdirs()
    out.resolve(name).writeText(text)
}

private fun dumpCommand(o: Options) {
    val results = o.classes.flatMap { classFiles(it) }.map { (rel, bytes) ->
        ClassResult(rel, bytes, extract(bytes), census(bytes), indy(bytes))
    }
    val allRefs = results.flatMap { it.refs }
    val extRefs = allRefs.filter { o.external.containsMatchIn(it.owner) }
    write(o.out, "refs-all.tsv", refsTsv(allRefs))
    write(o.out, "refs.tsv", refsTsv(extRefs))
    write(o.out, "tuples.tsv", tuplesTsv(tuples(extRefs)))
    write(o.out, "classes.tsv", censusTsv(results.map { it.census }))
    write(o.out, "cp-classes.tsv", cpClassesTsv(results.map { it.census }))
    write(o.out, "indy.tsv", indyTsv(results.flatMap { it.indy }))
    val summary = summaryMarkdown(o.label, results, o.external)
    write(o.out, "summary.md", summary)
    println(summary)
    println("[dump] ${results.size} classes from ${o.classes.joinToString { it.path }} -> ${o.out.path}")
}

private fun stripCommand(o: Options) {
    val classesOut = o.out.resolve("classes")
    val rows = ArrayList<Triple<String, Census, StripStats>>()
    o.classes.forEach { root ->
        classFiles(root).forEach { (rel, bytes) ->
            val (stripped, stats) = strip(bytes)
            val target = classesOut.resolve(rel)
            target.parentFile.mkdirs()
            target.writeBytes(stripped)
            val c = census(bytes)
            rows += Triple(c.className, c, stats)
        }
    }
    write(o.out, "strip-stats.tsv", stripStatsTsv(rows.map { it.first to it.third }))
    val summary = stripSummaryMarkdown(o.label, rows)
    write(o.out, "strip-summary.md", summary)
    println(summary)
    println("[strip] ${rows.size} classes -> ${classesOut.path}")
}
