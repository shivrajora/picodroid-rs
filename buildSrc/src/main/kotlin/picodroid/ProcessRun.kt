// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.GradleException

/**
 * Run a [ProcessBuilder], streaming its stdout/stderr to the console live
 * while also capturing stderr, so a failure can surface the actual error in
 * the [GradleException] message instead of just an exit code. Previously
 * these tasks used `inheritIO()` and threw a bare "(exit N)" — the real
 * cause (a panic, a bad map, a parse error) scrolled past in the build log
 * and was lost from the exception that CI / `--scan` reports.
 *
 * The builder is sanitized via [CargoEnv] before launch.
 */
object ProcessRun {
    /** Tail of captured stderr to embed in the exception (avoid huge messages). */
    private const val ERR_TAIL_LINES = 50

    fun runOrThrow(pb: ProcessBuilder, label: String) {
        CargoEnv.sanitize(pb)
        // Pipe stderr so we can capture it; let stdout inherit (progress).
        pb.redirectOutput(ProcessBuilder.Redirect.INHERIT)
        pb.redirectErrorStream(false)
        val proc = pb.start()

        // Drain stderr on a thread: echo each line live AND accumulate it.
        val captured = StringBuilder()
        val drainer = Thread {
            proc.errorStream.bufferedReader().forEachLine { line ->
                System.err.println(line)
                synchronized(captured) { captured.append(line).append('\n') }
            }
        }
        drainer.start()
        val rc = proc.waitFor()
        drainer.join()

        if (rc != 0) {
            val tail = synchronized(captured) {
                captured.toString().trimEnd().lines().takeLast(ERR_TAIL_LINES).joinToString("\n")
            }
            val detail = if (tail.isBlank()) "" else "\n--- stderr (last $ERR_TAIL_LINES lines) ---\n$tail"
            throw GradleException("$label failed (exit $rc)$detail")
        }
    }
}
