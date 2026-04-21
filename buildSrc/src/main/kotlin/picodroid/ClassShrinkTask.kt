package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.*

/**
 * Wraps `tools/class-shrink shrink-dir`. Only runs when a shrink map is
 * active (see [PicodroidPapkPlugin]); otherwise the task is wired to be
 * skipped and callers use the raw classes directory directly.
 */
abstract class ClassShrinkTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val inputDir: DirectoryProperty

    @get:InputFile
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val mapFile: RegularFileProperty

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:Input
    abstract val hostTarget: Property<String>

    @TaskAction
    fun run() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()

        val classShrinkManifest = project.rootDir.resolve("tools/class-shrink/Cargo.toml")
        val pb = ProcessBuilder(
            "cargo", "run", "--quiet",
            "--target", hostTarget.get(),
            "--manifest-path", classShrinkManifest.absolutePath,
            "--",
            "shrink-dir",
            "--in", inputDir.get().asFile.absolutePath,
            "--out", out.absolutePath,
            "--map", mapFile.get().asFile.absolutePath,
        ).directory(project.rootDir).inheritIO()
        CargoEnv.sanitize(pb)
        val rc = pb.start().waitFor()
        if (rc != 0) {
            throw GradleException("class-shrink shrink-dir failed (exit $rc)")
        }
    }
}
