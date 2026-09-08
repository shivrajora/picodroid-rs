rootProject.name = "picodroid"

include(":sdk")
// The hand-written kotlin/** stdlib shim that rides inside Kotlin apps' PAPKs
// (never in firmware). See docs/designs/kotlin-roadmap-2026-08.md.
include(":kotlin-shim")
// Compile-time DI: the javax.inject annotations (compileOnly on every Java
// app) and the annotation processor behind them. Host-only; nothing ships in
// firmware. See docs/designs/inject-annotations-2026-08.md.
include(":inject:annotations")
include(":inject:compiler")

// Auto-discover every examples/<name>/ that ships a PicodroidManifest.xml.
// Adding a new app requires no edit to this file — just create the dir +
// manifest + build.gradle.kts (or run ./gradlew newApp).
rootDir.resolve("examples").listFiles()
    ?.filter { it.isDirectory && it.resolve("PicodroidManifest.xml").isFile }
    ?.sortedBy { it.name }
    ?.forEach { include(":examples:${it.name}") }

// System apps (docs/designs/multi-app-2026-09.md D11): the launcher. Built
// like an example, then linked into every multi-app firmware by build.rs
// (scripts/lib.sh::build_system_apks). Same discovery rule as examples/.
rootDir.resolve("system-apps").listFiles()
    ?.filter { it.isDirectory && it.resolve("PicodroidManifest.xml").isFile }
    ?.sortedBy { it.name }
    ?.forEach { include(":system-apps:${it.name}") }
