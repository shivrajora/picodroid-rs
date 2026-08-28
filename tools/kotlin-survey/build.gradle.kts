// SPDX-License-Identifier: GPL-3.0-only
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension

plugins {
    id("org.jetbrains.kotlin.jvm") apply false
}

// The frozen flag string. docs/designs/kotlin-shim-inventory.md quotes this
// verbatim; Session 2's picodroid-papk-kotlin plugin copies it. Every flag is
// load-bearing for pico-jvm:
//   -Xjvm-default=all           real interface default methods, no DefaultImpls
//   -Xno-*-assertions           no Intrinsics.checkNotNullParameter per call
//   -Xstring-concat=inline      StringBuilder chains, never StringConcatFactory
//   -Xno-source-debug-extension no SMAP attribute from inline functions
//   -Xjdk-release=1.8           javac --release 8 parity: JDK 9+ APIs fail to compile
val kotlinFlags = listOf(
    "-Xjvm-default=all",
    "-Xno-param-assertions",
    "-Xno-call-assertions",
    "-Xno-receiver-assertions",
    "-Xstring-concat=inline",
    "-Xno-source-debug-extension",
    "-Xjdk-release=1.8",
)

subprojects {
    plugins.withId("org.jetbrains.kotlin.jvm") {
        // Uniform Java 1.8 compatibility on every subproject keeps
        // kotlin.jvm.target.validation.mode=error trivially satisfied.
        extensions.configure<JavaPluginExtension> {
            sourceCompatibility = JavaVersion.VERSION_1_8
            targetCompatibility = JavaVersion.VERSION_1_8
        }
        extensions.configure<KotlinJvmProjectExtension> {
            compilerOptions {
                jvmTarget.set(JvmTarget.JVM_1_8)
                allWarningsAsErrors.set(true)
                freeCompilerArgs.addAll(kotlinFlags)
            }
        }
    }
}

// One-shot regeneration of everything docs/designs/kotlin-shim-inventory.md
// quotes (the hello sim run is ./hello-sim.sh, outside Gradle on purpose).
tasks.register("survey") {
    group = "survey"
    description = "Regenerate every dump/strip output under out/"
    dependsOn(
        ":dump:dumpRefs", ":dump:dumpHello", ":dump:dumpJavaBaseline",
        ":dump:stripProto", ":dump:dumpStripped",
        ":hello:helloPapk", ":hello:helloPapkStripped",
    )
}
