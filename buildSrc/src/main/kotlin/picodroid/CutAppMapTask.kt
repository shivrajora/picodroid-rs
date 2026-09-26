// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.*

/**
 * Wraps `tools/class-shrink cut-app`: extends the active release shrink map
 * with this app's own rows, writing the merged map that `shrinkMembers`,
 * `shrinkClasses` and `packPapk` then consume in place of the release map.
 * Registered whenever a release map applies (see [PicodroidPapkPlugin]):
 * under `-Ppicodroid.shrinkApp=true` (`scripts/build-apk.sh --shrink-app`)
 * it renames the app's classes (`c/`) and private members; under plain
 * `--shrink` ([declashOnly]) it renames only the app names that would
 * collide with a shrunk framework name, so an app never has to rename
 * anything to be shrunk.
 *
 * Runs on the stripped, pre-shrink tree: original names, and for Kotlin apps
 * the staged shim too (kept by `sdk/keep.toml`'s `kotlin/…` glob, so it is
 * neither renamed nor mined for candidates). The merged map is the app's
 * retrace key — `scripts/build-apk.sh` copies it next to the PAPK.
 */
abstract class CutAppMapTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val inputDir: DirectoryProperty

    /** The active release map (`sdk/shrink-maps/v<version>.toml`). */
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val baseMapFile: RegularFileProperty

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val keepFile: RegularFileProperty

    /**
     * Name lists whose identifiers never become member targets:
     * `sdk/member-names.tsv` (every name the SDK declares) and
     * `sdk/api-contract.tsv` (every member the runtime serves).
     */
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val reserveNameFiles: ConfigurableFileCollection

    @get:OutputFile
    abstract val outputFile: RegularFileProperty

    /** Plain `--shrink`: rename only colliding app names (`cut-app --declash-only`). */
    @get:Input
    abstract val declashOnly: Property<Boolean>

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
        val classShrinkManifest = repoRoot.resolve("tools/class-shrink/Cargo.toml")
        val args = mutableListOf(
            "cargo", "run", "--quiet",
            "--target", hostTarget.get(),
            "--manifest-path", classShrinkManifest.absolutePath,
            "--",
            "cut-app",
            "--classes-dir", inputDir.get().asFile.absolutePath,
            "--base", baseMapFile.get().asFile.absolutePath,
            "--keep", keepFile.get().asFile.absolutePath,
            "--out", out.absolutePath,
        )
        reserveNameFiles.files.sortedBy { it.absolutePath }.forEach { args += listOf("--reserve-names", it.absolutePath) }
        if (declashOnly.get()) args += "--declash-only"
        val pb = ProcessBuilder(args).directory(repoRoot)
        ProcessRun.runOrThrow(pb, "class-shrink cut-app")
    }
}
