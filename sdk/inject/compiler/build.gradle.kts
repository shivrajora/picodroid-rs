plugins {
    `java-library`
}

// The javac annotation processor behind `@Inject` / `@Singleton`
// (docs/designs/inject-annotations-2026-08.md). Host-only: it rides the
// `annotationProcessor` configuration of every Java app (wired by
// PicodroidPapkPlugin) and emits plain Java 8 `Foo_Factory` /
// `Foo_MembersInjector` sources into the app's compile, where they are packed
// into the PAPK like any other app class. Zero runtime dependencies — the
// writers are StringBuilders — so the processor jar stays tiny and the
// generated output is byte-exact for the golden tests below.
dependencies {
    testImplementation("junit:junit:4.13.2")
    // Fixture compile classpath for the tests: the SDK (framework-component
    // detection needs picodroid.app.Activity & co.) and the annotations.
    testRuntimeOnly(project(":sdk"))
    testRuntimeOnly(project(":inject:annotations"))
}

tasks.test {
    useJUnit()
    // Hand the in-process javac the same classpath this test JVM runs with:
    // processor classes + SDK + annotations. Resolved at execution time so
    // configuration stays cheap.
    doFirst {
        systemProperty("picodroid.inject.testClasspath", sourceSets["test"].runtimeClasspath.asPath)
    }
}
