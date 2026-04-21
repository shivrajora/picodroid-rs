package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
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

    @get:OutputFile
    abstract val outputFile: RegularFileProperty

    @get:Input
    abstract val hostTarget: Property<String>

    @TaskAction
    fun run() {
        val out = outputFile.get().asFile
        out.parentFile.mkdirs()

        val manifest = project.rootDir.resolve("tools/papk-pack/Cargo.toml")
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

        val proc = ProcessBuilder(args).directory(project.rootDir).inheritIO().start()
        val rc = proc.waitFor()
        if (rc != 0) {
            throw GradleException("papk-pack failed (exit $rc)")
        }
    }
}
