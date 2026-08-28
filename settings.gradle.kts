rootProject.name = "picodroid"

include(":sdk")
// The hand-written kotlin/** stdlib shim that rides inside Kotlin apps' PAPKs
// (never in firmware). See docs/designs/kotlin-roadmap-2026-08.md.
include(":kotlin-shim")

// Auto-discover every examples/<name>/ that ships a PicodroidManifest.xml.
// Adding a new app requires no edit to this file — just create the dir +
// manifest + build.gradle.kts (or run ./gradlew newApp).
rootDir.resolve("examples").listFiles()
    ?.filter { it.isDirectory && it.resolve("PicodroidManifest.xml").isFile }
    ?.sortedBy { it.name }
    ?.forEach { include(":examples:${it.name}") }
