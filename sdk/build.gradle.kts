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
