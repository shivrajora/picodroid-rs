package picodroid

import org.gradle.api.GradleException
import java.io.File

/**
 * Determines which shrink map should be active for the current firmware
 * version. Mirrors the resolution done by `tools/class-shrink print-version`
 * and `build_support/papk.rs::resolve_active_map_version`, reimplemented in
 * pure Kotlin.
 *
 * Pure Kotlin on purpose — the earlier shell-out to `cargo run` deadlocked
 * when configuration ran inside a `cargo test` that already held the
 * target-dir lock.
 *
 * When [shrinkEnabled] is false, returns the `"0.0.0"` sentinel — same
 * convention as build_support/papk.rs.
 */
object ShrinkMapResolver {
    const val UNRELEASED: String = "0.0.0"

    private data class SemVer(val major: Int, val minor: Int, val patch: Int) : Comparable<SemVer> {
        override fun compareTo(other: SemVer): Int {
            val c1 = major.compareTo(other.major); if (c1 != 0) return c1
            val c2 = minor.compareTo(other.minor); if (c2 != 0) return c2
            return patch.compareTo(other.patch)
        }
    }

    fun resolve(repoRoot: File, shrinkEnabled: Boolean): String {
        if (!shrinkEnabled) return UNRELEASED
        val cargoToml = repoRoot.resolve("Cargo.toml")
        val shrinkMapsDir = repoRoot.resolve("sdk/shrink-maps")

        val pkgVersion = readPackageVersion(cargoToml)
            ?: throw GradleException("no [package] version in ${cargoToml.absolutePath}")
        val pkg = parseSemver(pkgVersion)
            ?: throw GradleException("unparseable [package] version '$pkgVersion' in ${cargoToml.absolutePath}")

        val candidates = mutableListOf<Pair<String, SemVer>>()
        for (file in shrinkMapsDir.listFiles() ?: emptyArray()) {
            val name = file.name
            if (!name.startsWith("v") || !name.endsWith(".toml")) continue
            val stem = name.removePrefix("v").removeSuffix(".toml")
            val sv = parseSemver(stem) ?: continue
            candidates += stem to sv
        }
        candidates.sortBy { it.second }
        return candidates.lastOrNull { it.second <= pkg }?.first ?: UNRELEASED
    }

    fun mapFile(repoRoot: File, version: String): File {
        val f = repoRoot.resolve("sdk/shrink-maps/v$version.toml")
        if (!f.isFile) {
            throw GradleException("active shrink map resolved to v$version but file missing: ${f.absolutePath}")
        }
        return f
    }

    /** Read `version = "x.y.z"` from the `[package]` section of Cargo.toml. */
    private fun readPackageVersion(cargoToml: File): String? {
        if (!cargoToml.isFile) return null
        var inPackage = false
        for (raw in cargoToml.readLines()) {
            val line = raw.trim()
            if (line.startsWith("[")) {
                inPackage = line == "[package]"
                continue
            }
            if (!inPackage) continue
            if (line.startsWith("version")) {
                val rest = line.substringAfter("version").trimStart().trimStart('=').trim()
                val v = rest.trim('"', '\'')
                if (parseSemver(v) != null) return v
            }
        }
        return null
    }

    /**
     * Accepts "x.y.z" plus optional `-pre` / `+build` suffix (stripped).
     * Returns null for malformed input. Matches build_support/papk.rs.
     */
    private fun parseSemver(s: String): SemVer? {
        val core = s.substringBefore('-').substringBefore('+')
        val parts = core.split('.')
        if (parts.size != 3) return null
        val a = parts[0].toIntOrNull() ?: return null
        val b = parts[1].toIntOrNull() ?: return null
        val c = parts[2].toIntOrNull() ?: return null
        return SemVer(a, b, c)
    }
}
