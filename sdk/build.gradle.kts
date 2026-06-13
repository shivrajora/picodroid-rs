plugins {
    `java-library`
}

sourceSets {
    main {
        java.setSrcDirs(listOf("java"))
        resources.setSrcDirs(emptyList<String>())
    }
    // Compile-only android.* stubs (see generateAndroidStubs below). Sources are
    // generated under build/; the set is self-contained (java.* only), so its
    // compile classpath is empty.
    create("androidStubs") {
        java.setSrcDirs(listOf(layout.buildDirectory.dir("generated/android-stubs")))
        compileClasspath = files()
        runtimeClasspath = files()
    }
}

// ── android.* compile stubs ───────────────────────────────────────────────
// Copy the picodroid.* sources with a textual `picodroid.` -> `android.`
// rename so app code can `import android.view.View` and compile. The bodies
// never run: these are compileOnly stubs. At runtime the real picodroid.*
// classes load, with app bytecode rewritten android/* -> picodroid/* by
// class-shrink's --compat-aliases pass (see sdk/compat-aliases.toml). The
// rename is self-consistent — every picodroid class moves together, so the
// stubs' cross-references stay valid. Generated every build, never committed.
val generateAndroidStubs by tasks.registering(Copy::class) {
    from("java/picodroid")
    into(layout.buildDirectory.dir("generated/android-stubs/android"))
    filter { line -> line.replace("picodroid.", "android.") }
}

tasks.named<JavaCompile>("compileAndroidStubsJava") {
    dependsOn(generateAndroidStubs)
}

val androidStubsJar by tasks.registering(Jar::class) {
    archiveBaseName.set("android-stubs")
    from(sourceSets["androidStubs"].output)
}

// Consumable configuration so app projects can put the stubs on their
// compileOnly classpath (PicodroidPapkPlugin wires this when compatAliases).
val androidStubsElements by configurations.creating {
    isCanBeConsumed = true
    isCanBeResolved = false
}
artifacts {
    add(androidStubsElements.name, androidStubsJar)
}
