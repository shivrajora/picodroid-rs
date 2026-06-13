// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.*

/**
 * Wraps `tools/papk-pack`. One of [mainClass], [activity], or [application]
 * is set by the plugin based on the parsed PicodroidManifest.xml.
 */
abstract class PapkPackTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val classesDir: DirectoryProperty

    @get:Input
    abstract val packageName: Property<String>

    @get:Input
    abstract val version: Property<String>

    @get:Input
    abstract val frameworkMapVersion: Property<String>

    @get:Input
    @get:Optional
    abstract val mainClass: Property<String>

    @get:Input
    @get:Optional
    abstract val activity: Property<String>

    @get:Input
    @get:Optional
    abstract val application: Property<String>

    @get:InputDirectory
    @get:Optional
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val assetsDir: DirectoryProperty

    @get:OutputFile
    abstract val outputFile: RegularFileProperty

    @get:Input
    abstract val hostTarget: Property<String>

    /** picodroid source tree (holds tools/); configurable for out-of-tree builds. */
    @get:Input
    abstract val repoRootPath: Property<String>

    @TaskAction
    fun run() {
        val out = outputFile.get().asFile
        out.parentFile.mkdirs()

        val repoRoot = java.io.File(repoRootPath.get())
        val manifest = repoRoot.resolve("tools/papk-pack/Cargo.toml")
        val args = mutableListOf(
            "cargo", "run", "--quiet",
            "--target", hostTarget.get(),
            "--manifest-path", manifest.absolutePath,
            "--",
        )
        mainClass.orNull?.let { args += listOf("--main-class", it) }
        activity.orNull?.let { args += listOf("--activity", it) }
        application.orNull?.let { args += listOf("--application", it) }
        args += listOf(
            "--package-name", packageName.get(),
            "--version", version.get(),
            "--framework-map-version", frameworkMapVersion.get(),
            "--classes-dir", classesDir.get().asFile.absolutePath,
            "--output", out.absolutePath,
        )
        assetsDir.orNull?.let { args += listOf("--assets-dir", it.asFile.absolutePath) }

        val pb = ProcessBuilder(args).directory(repoRoot)
        ProcessRun.runOrThrow(pb, "papk-pack")
    }
}
