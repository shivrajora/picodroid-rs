rootProject.name = "picodroid"

include(":sdk")

// Auto-discover every examples/<name>/ that ships a PicodroidManifest.xml.
// Adding a new app requires no edit to this file — just create the dir +
// manifest + build.gradle.kts (or run ./gradlew newApp).
rootDir.resolve("examples").listFiles()
    ?.filter { it.isDirectory && it.resolve("PicodroidManifest.xml").isFile }
    ?.sortedBy { it.name }
    ?.forEach { include(":examples:${it.name}") }
