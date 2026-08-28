// SPDX-License-Identifier: GPL-3.0-only
// Standalone survey build (docs/designs/kotlin-roadmap-2026-08.md, Session 1).
// Deliberately NOT included by the root settings.gradle.kts: it pulls the
// Kotlin Gradle plugin and the real kotlin-stdlib, which the app build must
// never see. Run it with the root wrapper:
//   ./gradlew -p tools/kotlin-survey survey
pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
    }
    plugins {
        // Frozen for the survey; a bump is its own roadmap Amendment (risk 19:
        // 2.2 renames -Xjvm-default and emits DefaultImpls by default).
        id("org.jetbrains.kotlin.jvm") version "2.1.21"
    }
}

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
}

rootProject.name = "kotlin-survey"

include(":fixture", ":hello", ":dump")
