// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.ListProperty
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

    /** `version-code` manifest key; the plugin always sets it (1 by default). */
    @get:Input
    abstract val versionCode: Property<Int>

    /** `label` manifest key; unset means the launcher shows the package name. */
    @get:Input
    @get:Optional
    abstract val label: Property<String>

    /** `icon` manifest key; must name a file in [assetsDir] (papk-pack checks). */
    @get:Input
    @get:Optional
    abstract val icon: Property<String>

    /** `design-width` / `design-height` manifest keys, from `<supports-screens>`; unset means resizeable. */
    @get:Input
    @get:Optional
    abstract val designWidth: Property<Int>

    @get:Input
    @get:Optional
    abstract val designHeight: Property<Int>

    /** `requires-features` manifest key: the `<uses-feature required="true">` names. */
    @get:Input
    abstract val requiresFeatures: ListProperty<String>

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

    /**
     * The class-shrink map [classesDir] was rewritten with, when shrinking is
     * on. papk-pack validates the entry point against descriptors as stored,
     * and a shrunk corpus spells `java/lang/String` as `b/…`.
     */
    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val shrinkMapFile: RegularFileProperty

    /** The app's `res/` tree, compiled into the RESOURCES section. */
    @get:InputDirectory
    @get:Optional
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val resDir: DirectoryProperty

    /**
     * The packer's sources — `tools/papk-pack`, `crates/papk-format`,
     * `crates/class-link`: a change to any of them changes what this task
     * writes, class files unchanged or not, so the output is stale without
     * them as inputs. Also what rebuilt the app's `R.java`, when it has one.
     */
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val packerSources: ConfigurableFileCollection

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
            "--version-code", versionCode.get().toString(),
            "--classes-dir", classesDir.get().asFile.absolutePath,
            "--output", out.absolutePath,
        )
        label.orNull?.let { args += listOf("--label", it) }
        icon.orNull?.let { args += listOf("--icon", it) }
        designWidth.orNull?.let { args += listOf("--design-size", "${it}x${designHeight.get()}") }
        requiresFeatures.get().forEach { args += listOf("--requires-feature", it) }
        assetsDir.orNull?.let { args += listOf("--assets-dir", it.asFile.absolutePath) }
        resDir.orNull?.let { args += listOf("--res-dir", it.asFile.absolutePath) }
        shrinkMapFile.orNull?.let { args += listOf("--shrink-map", it.asFile.absolutePath) }

        val pb = ProcessBuilder(args).directory(repoRoot)
        ProcessRun.runOrThrow(pb, "papk-pack")
    }
}
