// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.Plugin
import org.gradle.api.Project
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension

/**
 * `picodroid-papk-kotlin`: a Kotlin app under `examples/<app>/`. Applies the
 * Kotlin JVM plugin, then [PicodroidPapkPlugin], which inserts the
 * stage/strip stages when it sees the Kotlin plugin.
 *
 * Apps compile against the real `kotlin-stdlib` (KGP's default dependency,
 * never pinned separately) for type-checking only; what ships in the PAPK is
 * the app's classes plus the reachable part of `:kotlin-shim`, staged through
 * the `picodroidShim` configuration. The stdlib jar is never packed.
 */
class PicodroidPapkKotlinPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        target.plugins.apply("org.jetbrains.kotlin.jvm")

        val shimProjectPath = (target.findProperty("picodroid.shimProjectPath") as? String) ?: ":kotlin-shim"
        target.configurations.create(SHIM_CONFIGURATION) {
            isCanBeConsumed = false
            isCanBeResolved = true
            description = "Compiled kotlin-shim classes staged into this app's PAPK"
        }
        target.dependencies.add(
            SHIM_CONFIGURATION,
            target.dependencies.project(mapOf("path" to shimProjectPath, "configuration" to "shimClasses")),
        )

        target.plugins.apply(PicodroidPapkPlugin::class.java)

        val kotlin = target.extensions.getByType(KotlinJvmProjectExtension::class.java)
        kotlin.sourceSets.getByName("main").kotlin.apply {
            // Mirror the Java root: sources anywhere under the app dir.
            setSrcDirs(listOf(target.projectDir))
            include("**/*.kt")
            exclude("build/**")
        }
        kotlin.compilerOptions {
            jvmTarget.set(JvmTarget.JVM_1_8)
            allWarningsAsErrors.set(true)
            freeCompilerArgs.addAll(KOTLIN_FLAGS)
        }
    }

    companion object {
        const val SHIM_CONFIGURATION = "picodroidShim"

        /**
         * The frozen flag string (docs/designs/kotlin-shim-inventory.md header).
         * Every flag is load-bearing for pico-jvm: real default methods (no
         * DefaultImpls), no per-call null-check intrinsics, StringBuilder
         * concatenation, no SMAP annotation, JDK-8 API surface only.
         */
        val KOTLIN_FLAGS: List<String> = listOf(
            "-Xjvm-default=all",
            "-Xno-param-assertions",
            "-Xno-call-assertions",
            "-Xno-receiver-assertions",
            "-Xstring-concat=inline",
            "-Xno-source-debug-extension",
            "-Xjdk-release=1.8",
        )
    }
}
