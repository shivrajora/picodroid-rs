// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.plugins.JavaPlugin
import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.api.tasks.bundling.Jar
import org.gradle.api.tasks.compile.JavaCompile

/**
 * Picodroid .papk build plugin. Applied per-app under `examples/<app>/`.
 *
 * Pipeline: compileJava -> (optional) shrinkClasses -> packPapk.
 *
 * Shrinking gate: enabled by Gradle property `picodroid.shrink=true` or env
 * `PICODROID_SHRINK=1`. When enabled and a map is committed for the current
 * Cargo-root version, the map is applied; otherwise we pass the "0.0.0"
 * sentinel and skip the rewrite.
 */
class PicodroidPapkPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        target.plugins.apply(JavaPlugin::class.java)

        val javaExt = target.extensions.getByType(JavaPluginExtension::class.java)
        javaExt.sourceSets.getByName("main") {
            // Match scripts/build-apk.sh's `find APP_DIR -name '*.java'`:
            // some apps nest sources under `java/<pkg>/`, others drop .java
            // files directly into the project dir. Rooting at projectDir
            // with an include-filter handles both without per-app config.
            java.setSrcDirs(listOf(target.projectDir))
            java.include("**/*.java")
            java.exclude("build/**")
            resources.setSrcDirs(emptyList<String>())
        }

        target.dependencies.add(
            JavaPlugin.IMPLEMENTATION_CONFIGURATION_NAME,
            target.dependencies.project(mapOf("path" to ":sdk"))
        )

        // App jars are not published; skip the default jar task.
        target.tasks.named("jar", Jar::class.java) { enabled = false }

        val manifestFile = target.projectDir.resolve("PicodroidManifest.xml")
        val manifest = PicodroidManifest.parse(manifestFile)

        val shrinkEnabled = isShrinkEnabled(target)
        val frameworkMapVersion = target.rootProject.extra("picodroid.frameworkMapVersion") {
            ShrinkMapResolver.resolve(target.rootDir, shrinkEnabled)
        }
        // hostTarget is only needed by ClassShrinkTask + PapkPackTask — resolve
        // lazily via Provider so we don't shell out to rustc during plugin
        // configuration (keeps apply() fast and avoids any subprocess spawn
        // from a task-configuration path).
        val hostTarget = target.provider {
            target.rootProject.extra("picodroid.hostTarget") { HostTarget.detect() }
        }

        val compileJava = target.tasks.named("compileJava", JavaCompile::class.java)
        val classesOutputDir = compileJava.flatMap { it.destinationDirectory }

        val packClassesInput = if (frameworkMapVersion != ShrinkMapResolver.UNRELEASED) {
            val mapFile = ShrinkMapResolver.mapFile(target.rootDir, frameworkMapVersion)
            val shrinkTask = target.tasks.register("shrinkClasses", ClassShrinkTask::class.java) {
                dependsOn(compileJava)
                inputDir.set(classesOutputDir)
                this.mapFile.set(mapFile)
                outputDir.set(target.layout.buildDirectory.dir("classes-shrunk"))
                this.hostTarget.set(hostTarget)
            }
            shrinkTask.flatMap { it.outputDir }
        } else {
            classesOutputDir
        }

        // Per-app `assets/` directory is opt-in: present it to papk-pack only
        // when the dir actually exists, otherwise skip the flag entirely so
        // legacy v1.0 papks are emitted unchanged for apps without assets.
        val appAssetsDir = target.projectDir.resolve("assets")

        // When the app has assets, generate an AssetConstants.java so app code
        // can reference bundled files by a compile-checked constant instead of
        // a bare string literal. The generated source lives under build/ —
        // which the main srcDir excludes (exclude("build/**")) — so it must be
        // added as a SECOND srcDir, with compileJava depending on it.
        if (appAssetsDir.isDirectory) {
            val generatedSrcDir = target.layout.buildDirectory.dir("generated/picodroid-src")
            val genAssets = target.tasks.register(
                "generateAssetConstants", GenerateAssetConstantsTask::class.java
            ) {
                assetsDir.set(appAssetsDir)
                packageName.set(manifest.packageName)
                outputDir.set(generatedSrcDir)
            }
            javaExt.sourceSets.getByName("main").java.srcDir(genAssets.flatMap { it.outputDir })
            compileJava.configure { dependsOn(genAssets) }
        }

        val packPapk = target.tasks.register("packPapk", PapkPackTask::class.java) {
            classesDir.set(packClassesInput)
            packageName.set(target.name)
            version.set(manifest.version)
            this.frameworkMapVersion.set(frameworkMapVersion)
            manifest.mainClass?.let { mainClass.set(it) }
            manifest.activity?.let { activity.set(it) }
            manifest.application?.let { application.set(it) }
            if (appAssetsDir.isDirectory) {
                assetsDir.set(appAssetsDir)
            }
            outputFile.set(target.layout.buildDirectory.file("papk/${target.name}.papk"))
            this.hostTarget.set(hostTarget)
        }

        val assemblePapk = target.tasks.register("assemblePapk") {
            group = "build"
            description = "Produces a .papk firmware-embeddable bundle"
            dependsOn(packPapk)
        }
        target.tasks.named("assemble") { dependsOn(assemblePapk) }

        // Per-app run tasks. `sim` builds the papk (via assemblePapk) then runs
        // it in the host simulator; `install` pushes the papk to a connected
        // device with `pdb install`. Both reuse the Gradle-built papk rather
        // than rebuilding it (sim.sh would otherwise re-enter ./gradlew and
        // deadlock — see PICODROID_SKIP_GRADLE in scripts/build-apk.sh).
        target.tasks.register("sim", RunAppTask::class.java) {
            group = "picodroid"
            description = "Build and run ${target.name} in the host simulator"
            dependsOn(assemblePapk)
            mode.set("sim")
            appName.set(target.name)
            repoRootPath.set(target.rootDir.absolutePath)
        }
        target.tasks.register("install", RunAppTask::class.java) {
            group = "picodroid"
            description = "Build ${target.name} and push its papk to a connected device"
            dependsOn(assemblePapk)
            mode.set("install")
            appName.set(target.name)
            repoRootPath.set(target.rootDir.absolutePath)
            this.hostTarget.set(hostTarget)
            papkPath.set(packPapk.flatMap { it.outputFile }.map { it.asFile.absolutePath })
        }
    }

    private fun isShrinkEnabled(project: Project): Boolean {
        val prop = project.findProperty("picodroid.shrink") as? String
        if (prop != null) return prop.equals("true", ignoreCase = true) || prop == "1"
        val env = System.getenv("PICODROID_SHRINK")
        return env == "1"
    }

    /**
     * Memoized extra property on the root project — the resolver shells out
     * to cargo, so we do it once per configuration pass regardless of how
     * many app subprojects apply this plugin.
     */
    private inline fun <T> Project.extra(key: String, compute: () -> T): T {
        @Suppress("UNCHECKED_CAST")
        return if (extensions.extraProperties.has(key)) {
            extensions.extraProperties.get(key) as T
        } else {
            val v = compute()
            extensions.extraProperties.set(key, v)
            v
        }
    }
}
