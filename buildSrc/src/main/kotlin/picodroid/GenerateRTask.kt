// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.*

/**
 * Generates `R.java` from the app's `res/` directory — `R.string.*`,
 * `R.color.*`, `R.dimen.*`, `R.integer.*`, `R.bool.*`, `R.layout.*`,
 * `R.drawable.*`, `R.id.*` — by running `papk-pack gen-r`.
 *
 * The resource compiler lives in `tools/papk-pack` (module `res`) and is the
 * same code `packPapk --res-dir` runs over the same directory later, so the
 * ids compiled into the app and the table packed beside it cannot disagree.
 * Every field is a `static final int`: javac inlines them, nothing refers to
 * `R` at run time, and the plugin leaves the `R` classes out of the PAPK —
 * resources cost an app its table and nothing else.
 */
abstract class GenerateRTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val resDir: DirectoryProperty

    /** The app's Java package — its manifest `package=` — which `R` joins. */
    @get:Input
    abstract val packageName: Property<String>

    /**
     * The compiler's own sources: ids are whatever this code assigns, so a
     * changed compiler must regenerate `R.java` even over an unchanged `res/`.
     */
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val compilerSources: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:Input
    abstract val hostTarget: Property<String>

    @get:Input
    abstract val repoRootPath: Property<String>

    @TaskAction
    fun run() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()

        val repoRoot = java.io.File(repoRootPath.get())
        val args = listOf(
            "cargo", "run", "--quiet",
            "--target", hostTarget.get(),
            "--manifest-path", repoRoot.resolve("tools/papk-pack/Cargo.toml").absolutePath,
            "--",
            "gen-r",
            "--res-dir", resDir.get().asFile.absolutePath,
            "--package", packageName.get(),
            "--out-dir", out.absolutePath,
        )
        ProcessRun.runOrThrow(ProcessBuilder(args).directory(repoRoot), "papk-pack gen-r")
    }
}
