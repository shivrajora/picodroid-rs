// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import picodroid.classfile.MemberRemapper
import picodroid.classfile.ShrinkMapMembers
import picodroid.classfile.shrinkMembers

/**
 * Applies the active shrink map's `[[member]]` renames to every `.class`
 * under [inputDir], writing each to the same relative path under
 * [outputDir] (class names are untouched here — `shrinkClasses` /
 * `class-shrink shrink-dir` renames those afterwards).
 *
 * Runs for app PAPKs (between the optional strip and `shrinkClasses`, see
 * [PicodroidPapkPlugin]) and for the SDK corpus (`:sdk:shrinkMembersStripped`
 * / `:sdk:shrinkMembersRaw`, invoked by `build_support/papk.rs` when the
 * firmware is built with `PICODROID_SHRINK=1` and the map has members).
 * A map without `[[member]]` rows copies the tree through unchanged.
 *
 * Rust matches SDK method names through the generated `shrink_names::m`
 * consts (docs/designs/flash-string-budget-2026-08.md §5.2), so the same map
 * that rewrites the bytecode here rewrites what the native handlers expect.
 */
abstract class ShrinkMembersTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val inputDir: DirectoryProperty

    @get:InputFile
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val mapFile: RegularFileProperty

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @TaskAction
    fun run() {
        val input = inputDir.get().asFile
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        val members = ShrinkMapMembers.parse(mapFile.get().asFile)
        val remapper = MemberRemapper(members)
        var classes = 0
        var bytesIn = 0L
        var bytesOut = 0L
        input.walkTopDown().filter { it.isFile && it.extension == "class" }.forEach { file ->
            val rel = file.relativeTo(input).path
            val target = out.resolve(rel)
            target.parentFile.mkdirs()
            val bytes = file.readBytes()
            val shrunk = if (members.isEmpty()) bytes else shrinkMembers(bytes, remapper)
            target.writeBytes(shrunk)
            classes++
            bytesIn += bytes.size
            bytesOut += shrunk.size
        }
        logger.lifecycle(
            "shrinkMembers: $classes classes, ${members.size} member renames, $bytesIn -> $bytesOut bytes"
        )
    }
}
