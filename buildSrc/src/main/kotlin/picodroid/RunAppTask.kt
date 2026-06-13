// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.TaskAction

/**
 * Runs a built app, either in the host simulator (`mode = "sim"`) or by
 * pushing its papk to a connected device (`mode = "install"`). These are
 * interactive "run" tasks — they always execute (never up-to-date) and
 * stream their output straight to the console.
 *
 * Both modes depend on the app's papk already being built by the SAME
 * Gradle invocation (the registering plugin wires `dependsOn(assemblePapk)`),
 * so the sim path sets `PICODROID_SKIP_GRADLE=1`: re-entering `./gradlew`
 * from inside a running build would deadlock on the project lock.
 */
abstract class RunAppTask : DefaultTask() {
    @get:Internal
    abstract val appName: Property<String>

    /** "sim" or "install". */
    @get:Internal
    abstract val mode: Property<String>

    @get:Internal
    abstract val repoRootPath: Property<String>

    /** Host target triple — only needed by the install (pdb) path. */
    @get:Internal
    abstract val hostTarget: Property<String>

    /** Absolute path to the built papk — only needed by the install path. */
    @get:Internal
    abstract val papkPath: Property<String>

    init {
        // Run tasks have no cacheable output; always execute.
        outputs.upToDateWhen { false }
    }

    @TaskAction
    fun run() {
        val root = java.io.File(repoRootPath.get())
        val app = appName.get()
        val pb = when (val m = mode.get()) {
            "sim" -> ProcessBuilder(
                "bash", root.resolve("scripts/sim.sh").absolutePath, "--app", app
            ).also { it.environment()["PICODROID_SKIP_GRADLE"] = "1" }

            "install" -> ProcessBuilder(
                "cargo", "run", "--quiet",
                "--target", hostTarget.get(),
                "--manifest-path", root.resolve("tools/pdb/Cargo.toml").absolutePath,
                "--", "install", papkPath.get(),
            )

            else -> throw GradleException("RunAppTask: unknown mode '$m'")
        }
        pb.directory(root)
        CargoEnv.sanitize(pb)
        // Merge stderr into stdout and drain it on this thread, echoing each
        // line via System.out. A subprocess that inheritIO()s under the Gradle
        // daemon writes to the daemon's FDs (not the client console), so its
        // output is lost; routing through System.out lets Gradle forward it to
        // whoever invoked the build. Streams live, so a long-running sim shows
        // output as it happens.
        pb.redirectErrorStream(true)

        val proc = pb.start()
        proc.inputStream.bufferedReader().forEachLine { println(it) }
        val rc = proc.waitFor()
        if (rc != 0) {
            throw GradleException("picodroid ${mode.get()} for '$app' exited with $rc")
        }
    }
}
