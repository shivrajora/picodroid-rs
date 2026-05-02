// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.GradleException
import java.io.ByteArrayOutputStream

object HostTarget {
    fun detect(): String {
        val out = ByteArrayOutputStream()
        val proc = ProcessBuilder("rustc", "-vV").redirectErrorStream(true).start()
        proc.inputStream.copyTo(out)
        val rc = proc.waitFor()
        if (rc != 0) {
            throw GradleException("rustc -vV failed (exit $rc):\n${out.toString(Charsets.UTF_8)}")
        }
        return out.toString(Charsets.UTF_8)
            .lineSequence()
            .firstOrNull { it.startsWith("host:") }
            ?.substringAfter("host:")
            ?.trim()
            ?: throw GradleException("could not parse host target from rustc -vV output")
    }
}
