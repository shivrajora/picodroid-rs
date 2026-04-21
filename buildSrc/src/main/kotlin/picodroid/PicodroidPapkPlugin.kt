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
        val hostTarget = target.rootProject.extra("picodroid.hostTarget") { HostTarget.detect() }
        val frameworkMapVersion = target.rootProject.extra("picodroid.frameworkMapVersion") {
            ShrinkMapResolver.resolve(target.rootDir, hostTarget, shrinkEnabled)
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

        val packPapk = target.tasks.register("packPapk", PapkPackTask::class.java) {
            classesDir.set(packClassesInput)
            packageName.set(target.name)
            version.set(manifest.version)
            this.frameworkMapVersion.set(frameworkMapVersion)
            manifest.mainClass?.let { mainClass.set(it) }
            manifest.activity?.let { activity.set(it) }
            manifest.application?.let { application.set(it) }
            outputFile.set(target.layout.buildDirectory.file("papk/${target.name}.papk"))
            this.hostTarget.set(hostTarget)
        }

        val assemblePapk = target.tasks.register("assemblePapk") {
            group = "build"
            description = "Produces a .papk firmware-embeddable bundle"
            dependsOn(packPapk)
        }
        target.tasks.named("assemble") { dependsOn(assemblePapk) }
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
