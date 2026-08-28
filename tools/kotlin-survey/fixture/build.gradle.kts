// SPDX-License-Identifier: GPL-3.0-only
// The picoenvmon-shaped survey fixture. Compiled against the real
// kotlin-stdlib (KGP default, never pinned) plus the SDK's compiled classes;
// the output is dumped by :dump and never executed anywhere.
plugins {
    id("org.jetbrains.kotlin.jvm")
}

val sdkClasses = rootDir.parentFile.parentFile.resolve("sdk/build/classes/java/main")

dependencies {
    compileOnly(files(sdkClasses))
}

tasks.named("compileKotlin") {
    doFirst {
        if (!sdkClasses.resolve("picodroid/util/Log.class").isFile) {
            throw GradleException(
                "SDK classes missing at $sdkClasses — run ./gradlew :sdk:compileJava at the repo root"
            )
        }
    }
}
