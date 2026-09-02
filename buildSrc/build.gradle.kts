plugins {
    `kotlin-dsl`
}

repositories {
    mavenCentral()
}

dependencies {
    // `implementation`, never `compileOnly`: plugin-classloader isolation would
    // hide a compileOnly KGP from PicodroidPapkKotlinPlugin at run time. The
    // version is the Kotlin pin for every Kotlin app (docs/designs/kotlin-roadmap-2026-08.md).
    implementation("org.jetbrains.kotlin:kotlin-gradle-plugin:2.1.21")
    // Class-file strip/prune/shake for Kotlin apps (picodroid.classfile.*).
    implementation("org.ow2.asm:asm:9.7")
    // ClassRemapper for the member-name shrink (ShrinkMembersTask).
    implementation("org.ow2.asm:asm-commons:9.7")
}

gradlePlugin {
    plugins {
        create("picodroid-papk") {
            id = "picodroid-papk"
            implementationClass = "picodroid.PicodroidPapkPlugin"
        }
        create("picodroid-papk-kotlin") {
            id = "picodroid-papk-kotlin"
            implementationClass = "picodroid.PicodroidPapkKotlinPlugin"
        }
    }
}
