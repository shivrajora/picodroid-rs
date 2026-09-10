plugins {
    `java-library`
}

// JSR-330 annotation types (`javax.inject.Inject` / `Singleton` / `Scope`)
// consumed at compile time by :inject:compiler. SOURCE retention, compileOnly
// on every Java app: nothing here ever reaches a PAPK or the firmware
// (docs/designs/inject-annotations-2026-08.md).
sourceSets {
    main {
        java.setSrcDirs(listOf("java"))
        resources.setSrcDirs(emptyList<String>())
    }
}
