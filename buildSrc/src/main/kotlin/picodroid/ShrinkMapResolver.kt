package picodroid

import org.gradle.api.GradleException
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Determines which shrink map should be active for the current firmware
 * version. Mirrors the resolution done by `tools/class-shrink print-version`,
 * which is what scripts/build-apk.sh calls today. We shell out to the same
 * binary so the two sides can't drift.
 *
 * When [shrinkEnabled] is false, returns the `"0.0.0"` sentinel — same
 * convention as build_support/papk.rs.
 */
object ShrinkMapResolver {
    const val UNRELEASED: String = "0.0.0"

    fun resolve(
        repoRoot: File,
        hostTarget: String,
        shrinkEnabled: Boolean,
    ): String {
        if (!shrinkEnabled) return UNRELEASED
        val cargoToml = repoRoot.resolve("Cargo.toml")
        val shrinkMapsDir = repoRoot.resolve("sdk/shrink-maps")
        val manifest = repoRoot.resolve("tools/class-shrink/Cargo.toml")

        val out = ByteArrayOutputStream()
        val err = ByteArrayOutputStream()
        val proc = ProcessBuilder(
            "cargo", "run", "--quiet",
            "--target", hostTarget,
            "--manifest-path", manifest.absolutePath,
            "--",
            "print-version",
            "--cargo-toml", cargoToml.absolutePath,
            "--shrink-maps-dir", shrinkMapsDir.absolutePath,
        ).directory(repoRoot).start()
        proc.inputStream.copyTo(out)
        proc.errorStream.copyTo(err)
        val rc = proc.waitFor()
        if (rc != 0) {
            throw GradleException(
                "class-shrink print-version failed (exit $rc):\n${err.toString(Charsets.UTF_8)}"
            )
        }
        return out.toString(Charsets.UTF_8).trim()
    }

    fun mapFile(repoRoot: File, version: String): File {
        val f = repoRoot.resolve("sdk/shrink-maps/v$version.toml")
        if (!f.isFile) {
            throw GradleException("active shrink map resolved to v$version but file missing: ${f.absolutePath}")
        }
        return f
    }
}
