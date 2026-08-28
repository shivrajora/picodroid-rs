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
