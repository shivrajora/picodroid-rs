// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import picodroid.classfile.strip

/**
 * Runs [strip] over every `.class` under [inputDir] and writes the result to
 * the same relative path under [outputDir] — a plain per-class map, without
 * [StripClassMetadataTask]'s Kotlin-only shim pruning and shaking.
 *
 * Two users, both device-bound: `:sdk:stripClasses` produces the tree
 * `build.rs` embeds when the firmware is built with `debug_assertions` off
 * (every device build), and [PicodroidPapkPlugin] inserts it into a Java app's
 * pipeline under `-Ppicodroid.stripDebug=true`. With [keepLineNumbers] false
 * the `LineNumberTable` and `SourceFile` attributes go too: pico-jvm reads
 * them only in `debug_assertions` builds (the host simulator's `(:line)` stack
 * traces), so device images carried them for nothing
 * (docs/designs/flash-string-budget-2026-08.md §4).
 *
 * The output is `ClassWriter(0)` without a reader, so the constant pool is
 * rebuilt without the dropped attributes' orphaned Utf8 entries. HotSpot would
 * reject it (no StackMapTable); nothing but pico-jvm may load this tree.
 */
abstract class ClassStripTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val inputDir: DirectoryProperty

    /** `false` also drops `LineNumberTable` + `SourceFile`. An `@Input`, so a flip re-runs the task. */
    @get:Input
    abstract val keepLineNumbers: Property<Boolean>

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    init {
        keepLineNumbers.convention(true)
    }

    @TaskAction
    fun run() {
        val input = inputDir.get().asFile
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        val keep = keepLineNumbers.get()
        var classes = 0
        var bytesIn = 0
        var bytesOut = 0
        var cpIn = 0
        var cpOut = 0
        input.walkTopDown()
            .filter { it.isFile && it.name.endsWith(".class") }
            .sortedBy { it.relativeTo(input).invariantSeparatorsPath }
            .forEach { f ->
                val (bytes, stats) = strip(f.readBytes(), keepLineNumbers = keep)
                val target = out.resolve(f.relativeTo(input).path)
                target.parentFile.mkdirs()
                target.writeBytes(bytes)
                classes++
                bytesIn += stats.bytesBefore
                bytesOut += stats.bytesAfter
                cpIn += stats.cpBefore
                cpOut += stats.cpAfter
            }
        logger.lifecycle(
            "[${project.name}] strip: $classes classes, $bytesIn -> $bytesOut bytes, " +
                "$cpIn -> $cpOut CP entries, lineNumbers=$keep"
        )
    }
}
