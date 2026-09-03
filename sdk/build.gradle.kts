import org.gradle.api.tasks.compile.JavaCompile

plugins {
    `java-library`
}

sourceSets {
    main {
        java.setSrcDirs(listOf("java"))
        resources.setSrcDirs(emptyList<String>())
    }
}

// Firmware and the sim embed one of THESE trees, never compileJava's: build.rs
// picks by the `line-numbers` cargo feature (CARGO_FEATURE_LINE_NUMBERS). Both
// drop everything pico-jvm skips by length (StackMapTable, annotations,
// Signature, …, docs/designs/flash-string-budget-2026-08.md §4);
// `stripClassesLines` keeps LineNumberTable + SourceFile for a JVM that prints
// `(File.java:39)` frames (the sim, debug-profile flash.sh firmware), and
// `stripClasses` drops those too for release firmware, where they would be
// ~15 KB of dead flash. Separate directories so one build's Gradle run can
// never rewrite what another has already include_bytes!'d. Nothing but
// build.rs may consume them — HotSpot rejects frame-less classes.
val stripClasses by tasks.registering(picodroid.ClassStripTask::class) {
    description = "Strip debug attributes from the SDK classes for release firmware embedding"
    inputDir.set(tasks.named<JavaCompile>("compileJava").flatMap { it.destinationDirectory })
    keepLineNumbers.set(false)
    outputDir.set(layout.buildDirectory.dir("classes-stripped/java/main"))
}
val stripClassesLines by tasks.registering(picodroid.ClassStripTask::class) {
    description = "Strip the SDK classes but keep LineNumberTable/SourceFile, for line-numbers firmware"
    inputDir.set(tasks.named<JavaCompile>("compileJava").flatMap { it.destinationDirectory })
    keepLineNumbers.set(true)
    outputDir.set(layout.buildDirectory.dir("classes-stripped-lines/java/main"))
}

// Member-name shrink of the SDK corpus (docs/designs/flash-string-budget-2026-08.md
// §5.2). build.rs invokes one of these — with `-Ppicodroid.shrinkMap=<abs path>`
// — when the firmware is built with PICODROID_SHRINK=1 and the active map has
// `[[member]]` rows, then runs the Rust class-name pass on the output. One
// variant per strip tree above, with distinct output directories so neither
// build can overwrite what the other embedded.
val shrinkMapFile = layout.file(providers.gradleProperty("picodroid.shrinkMap").map { File(it) })
val shrinkMembersStripped by tasks.registering(picodroid.ShrinkMembersTask::class) {
    description = "Apply the shrink map's member renames to the stripped SDK classes"
    inputDir.set(stripClasses.flatMap { it.outputDir })
    mapFile.set(shrinkMapFile)
    outputDir.set(layout.buildDirectory.dir("classes-members-stripped/java/main"))
}
val shrinkMembersStrippedLines by tasks.registering(picodroid.ShrinkMembersTask::class) {
    description = "Apply the shrink map's member renames to the stripped-with-lines SDK classes"
    inputDir.set(stripClassesLines.flatMap { it.outputDir })
    mapFile.set(shrinkMapFile)
    outputDir.set(layout.buildDirectory.dir("classes-members-stripped-lines/java/main"))
}
