// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.*

/**
 * Wraps `tools/class-shrink shrink-dir`. Runs when a shrink map is active
 * and/or compat-aliases are requested (see [PicodroidPapkPlugin]); otherwise
 * the task is skipped and callers use the raw classes directory directly.
 *
 * At least one of [mapFile] / [compatAliasesFile] must be set. The alias pass
 * (android.* to picodroid.*) is composed ahead of the shrink rewrite by the
 * tool, so a single invocation covers both.
 */
abstract class ClassShrinkTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val inputDir: DirectoryProperty

    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val mapFile: RegularFileProperty

    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val compatAliasesFile: RegularFileProperty

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:Input
    abstract val hostTarget: Property<String>

    /** picodroid source tree (holds tools/); configurable for out-of-tree builds. */
    @get:Input
    abstract val repoRootPath: Property<String>

    @TaskAction
    fun run() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()

        val repoRoot = java.io.File(repoRootPath.get())
        val classShrinkManifest = repoRoot.resolve("tools/class-shrink/Cargo.toml")
        val args = mutableListOf(
            "cargo", "run", "--quiet",
            "--target", hostTarget.get(),
            "--manifest-path", classShrinkManifest.absolutePath,
            "--",
            "shrink-dir",
            "--in", inputDir.get().asFile.absolutePath,
            "--out", out.absolutePath,
        )
        mapFile.orNull?.let { args += listOf("--map", it.asFile.absolutePath) }
        compatAliasesFile.orNull?.let { args += listOf("--compat-aliases", it.asFile.absolutePath) }

        val pb = ProcessBuilder(args).directory(repoRoot)
        ProcessRun.runOrThrow(pb, "class-shrink shrink-dir")
    }
}
