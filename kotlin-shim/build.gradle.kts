plugins {
    `java-library`
}

// Hand-written Java implementations of the kotlin/** classes kotlinc-compiled
// apps reference (docs/designs/kotlin-shim-inventory.md is the source of
// truth). Compiled against the SDK only; never on an app's compile classpath
// (apps type-check against the real kotlin-stdlib) and never in firmware —
// the compiled classes are staged into each Kotlin app's PAPK, where the
// strip task prunes what the app cannot reach.
sourceSets {
    main {
        java.setSrcDirs(listOf("java"))
        resources.setSrcDirs(emptyList<String>())
    }
}

dependencies {
    compileOnly(project(":sdk"))
}

// Outgoing variant consumed by picodroid-papk-kotlin's `picodroidShim`
// configuration: the classes directory, not a jar.
val shimClasses: Configuration by configurations.creating {
    isCanBeConsumed = true
    isCanBeResolved = false
}

artifacts {
    add("shimClasses", layout.buildDirectory.dir("classes/java/main")) {
        builtBy(tasks.named("compileJava"))
    }
}

// ── Contract check (roadmap Session 5) ──────────────────────────────────────
// Every kotlin/** reference the fixture apps make must resolve in this shim
// (Direction A), unused shim members are reported (B), and every java/**
// reference — from the shim and from the fixtures — must be a row of
// jdk-allowlist.tsv (C), which picodroid-core's `jdk_allowlist_owners_are_served`
// test cross-checks against the JVM's builtin tables.
val shimFixtures: Configuration by configurations.creating {
    isCanBeConsumed = false
    isCanBeResolved = true
    description = "Compiled classes of the Kotlin apps that define the shim's required surface"
}

dependencies {
    shimFixtures(project(mapOf("path" to ":examples:langsuite_kt", "configuration" to "picodroidAppClasses")))
    shimFixtures(project(mapOf("path" to ":examples:langsuite_kt_stdlib", "configuration" to "picodroidAppClasses")))
    // The @Inject twin: kapt-generated factories/injectors plus the Kotlin DI
    // shapes (lateinit fields, object modules) get Direction A/C coverage.
    shimFixtures(project(mapOf("path" to ":examples:injectdemo_kt", "configuration" to "picodroidAppClasses")))
}

val contractCheck by tasks.registering(picodroid.ShimContractTask::class) {
    group = "verification"
    description = "kotlin-shim contract: fixture kotlin/** refs resolve, unused members, JDK allowlist"
    shimClasses.set(tasks.named<JavaCompile>("compileJava").flatMap { it.destinationDirectory })
    fixtureClasses.from(shimFixtures)
    allowlistFile.set(layout.projectDirectory.file("jdk-allowlist.tsv"))
    // Direction B is an error since Session 6 (tiers 0-2 shipped with an empty
    // unused list): a shim member no fixture references is dead weight in every
    // Kotlin PAPK, so add the demo check first, then the member.
    strictUnused.set(true)
    reportFile.set(layout.buildDirectory.file("reports/shim-contract.txt"))
}

tasks.named("check") { dependsOn(contractCheck) }
