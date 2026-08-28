// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import picodroid.classfile.ClassEntry
import picodroid.classfile.ShimShaker

/**
 * Kotlin-app class pipeline stage: `stageClasses` (app + shim classes) →
 * strip metadata, apply `@ShimName`, prune unreachable `kotlin/…` classes,
 * shake unreferenced `*Kt` statics ([ShimShaker]) → `classes-stripped/`.
 * Java-only apps never run this task (their PAPKs stay byte-identical).
 */
abstract class StripClassMetadataTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val inputDir: DirectoryProperty

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:OutputFile
    abstract val reportFile: RegularFileProperty

    @TaskAction
    fun run() {
        val input = inputDir.get().asFile
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        val entries = input.walkTopDown()
            .filter { it.isFile && it.name.endsWith(".class") }
            .map { ClassEntry(it.relativeTo(input).invariantSeparatorsPath, it.readBytes()) }
            .sortedBy { it.relPath }
            .toList()
        val (kept, report) = ShimShaker.process(entries)
        kept.forEach { e ->
            val f = out.resolve(e.relPath)
            f.parentFile.mkdirs()
            f.writeBytes(e.bytes)
        }
        val text = report.render()
        reportFile.get().asFile.apply { parentFile.mkdirs(); writeText(text) }
        logger.lifecycle("[${project.name}] " + text.trimEnd().replace("\n", "\n[${project.name}] "))
    }
}
