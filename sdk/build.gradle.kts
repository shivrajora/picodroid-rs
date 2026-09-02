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

// Device firmware embeds THIS tree, not compileJava's: build.rs selects it when
// the firmware is built with debug_assertions off (every device build), because
// pico-jvm reads LineNumberTable only in debug_assertions builds and never
// reads SourceFile/StackMapTable (docs/designs/flash-string-budget-2026-08.md
// §4). A separate directory so a sim build's Gradle run can never rewrite what
// a firmware build has already include_bytes!'d. Nothing but build.rs may
// consume it — HotSpot rejects frame-less classes.
val stripClasses by tasks.registering(picodroid.ClassStripTask::class) {
    description = "Strip debug attributes from the SDK classes for firmware embedding"
    inputDir.set(tasks.named<JavaCompile>("compileJava").flatMap { it.destinationDirectory })
    keepLineNumbers.set(false)
    outputDir.set(layout.buildDirectory.dir("classes-stripped/java/main"))
}

// Member-name shrink of the SDK corpus (docs/designs/flash-string-budget-2026-08.md
// §5.2). build.rs invokes one of these — with `-Ppicodroid.shrinkMap=<abs path>`
// — when the firmware is built with PICODROID_SHRINK=1 and the active map has
// `[[member]]` rows, then runs the Rust class-name pass on the output. Two
// variants because the input differs by build: the stripped tree for device
// firmware, compileJava's raw tree for debug_assertions (sim) builds; distinct
// output directories so neither build can overwrite what the other embedded.
val shrinkMapFile = layout.file(providers.gradleProperty("picodroid.shrinkMap").map { File(it) })
val shrinkMembersStripped by tasks.registering(picodroid.ShrinkMembersTask::class) {
    description = "Apply the shrink map's member renames to the stripped SDK classes"
    inputDir.set(stripClasses.flatMap { it.outputDir })
    mapFile.set(shrinkMapFile)
    outputDir.set(layout.buildDirectory.dir("classes-members-stripped/java/main"))
}
val shrinkMembersRaw by tasks.registering(picodroid.ShrinkMembersTask::class) {
    description = "Apply the shrink map's member renames to the raw SDK classes"
    inputDir.set(tasks.named<JavaCompile>("compileJava").flatMap { it.destinationDirectory })
    mapFile.set(shrinkMapFile)
    outputDir.set(layout.buildDirectory.dir("classes-members-raw/java/main"))
}
