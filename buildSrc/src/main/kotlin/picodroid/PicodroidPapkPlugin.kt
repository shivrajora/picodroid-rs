// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.plugins.JavaPlugin
import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.api.file.Directory
import org.gradle.api.provider.Provider
import org.gradle.api.tasks.Sync
import org.gradle.api.tasks.bundling.Jar
import org.gradle.api.tasks.compile.JavaCompile
import org.jetbrains.kotlin.gradle.tasks.KotlinCompile
import java.io.File

/**
 * Picodroid .papk build plugin. Applied per-app under `examples/<app>/`.
 *
 * Pipeline: compileJava -> verifyApiContract -> (optional) shrinkClasses ->
 * packPapk. Kotlin apps (`picodroid-papk-kotlin`): kapt (stubs + @Inject
 * processor) -> compileKotlin + compileJava -> stageClasses (+ shim) ->
 * stripClassMetadata -> verifyApiContract -> (optional) shrinkClasses ->
 * packPapk. Java-only apps take the first path untouched.
 *
 * `verifyApiContract` rejects java/… references pico-jvm does not serve
 * (the generated sdk/api-contract.tsv) and, with `-Ppicodroid.board=<name>`,
 * classes that board excludes from its framework. `-Ppicodroid.apiContract=`
 * `error` (default) | `warn` | `off`.
 *
 * Shrinking gate: enabled by Gradle property `picodroid.shrink=true` or env
 * `PICODROID_SHRINK=1`. When enabled and a map is committed for the current
 * Cargo-root version, the map is applied; otherwise we pass the "0.0.0"
 * sentinel and skip the rewrite.
 */
class PicodroidPapkPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        target.plugins.apply(JavaPlugin::class.java)

        // Roots are configurable so an app project can build out-of-tree
        // against a picodroid checkout it isn't a subproject of:
        //   picodroid.repoRoot       — the picodroid source tree (holds tools/,
        //                              sdk/shrink-maps/, platforms/, scripts/).
        //                              Default: this build's root project dir.
        //   picodroid.sdkProjectPath — Gradle path of the :sdk project to
        //                              compile against. Default ":sdk".
        // No publishing/template is involved — only path indirection.
        val repoRoot: File = (target.findProperty("picodroid.repoRoot") as? String)
            ?.let { target.file(it) }
            ?: target.rootProject.rootDir
        val sdkProjectPath = (target.findProperty("picodroid.sdkProjectPath") as? String) ?: ":sdk"

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
            target.dependencies.project(mapOf("path" to sdkProjectPath))
        )

        // Compile-time DI (docs/designs/inject-annotations-2026-08.md): the
        // javax.inject annotations ride compileOnly (SOURCE retention — never
        // in a PAPK) and the processor emits *_Factory / *_MembersInjector
        // sources, compiled and packed with the app's own classes. Java apps
        // run it through javac's annotationProcessor path; Kotlin apps run the
        // same processor through kapt (applied by picodroid-papk-kotlin before
        // this plugin), which processes kotlinc's stubs plus any Java sources
        // and hands the generated Java to compileJava — kapt forces
        // `-proc:none` on compileJava, so the processor must ride the `kapt`
        // configuration there. compileOnly is on the compile classpath both
        // kotlinc and the stub pass consume, so no separate Kotlin dependency
        // is needed. Paths mirror sdkProjectPath so an out-of-tree app can
        // point at its picodroid checkout.
        val injectAnnotationsPath =
            (target.findProperty("picodroid.injectAnnotationsProjectPath") as? String) ?: ":inject:annotations"
        val injectCompilerPath =
            (target.findProperty("picodroid.injectCompilerProjectPath") as? String) ?: ":inject:compiler"
        target.dependencies.add(
            JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME,
            target.dependencies.project(mapOf("path" to injectAnnotationsPath))
        )
        val processorConfiguration =
            if (target.plugins.hasPlugin("org.jetbrains.kotlin.kapt")) "kapt"
            else JavaPlugin.ANNOTATION_PROCESSOR_CONFIGURATION_NAME
        target.dependencies.add(
            processorConfiguration,
            target.dependencies.project(mapOf("path" to injectCompilerPath))
        )

        // App jars are not published; skip the default jar task.
        target.tasks.named("jar", Jar::class.java) { enabled = false }

        val manifestFile = target.projectDir.resolve("PicodroidManifest.xml")
        val manifest = PicodroidManifest.parse(manifestFile)

        val shrinkEnabled = isShrinkEnabled(target)
        val frameworkMapVersion = target.rootProject.extra("picodroid.frameworkMapVersion") {
            ShrinkMapResolver.resolve(repoRoot, shrinkEnabled)
        }
        // hostTarget is only needed by ClassShrinkTask + PapkPackTask — resolve
        // lazily via Provider so we don't shell out to rustc during plugin
        // configuration (keeps apply() fast and avoids any subprocess spawn
        // from a task-configuration path).
        val hostTarget = target.provider {
            target.rootProject.extra("picodroid.hostTarget") { HostTarget.detect() }
        }

        val compileJava = target.tasks.named("compileJava", JavaCompile::class.java)
        // Full recompiles only: the generated-source roots below (assets,
        // net-test config) live under build/, which nests them inside the
        // projectDir source root — overlapping roots break Gradle's
        // incremental source-to-class mapping (a regenerated file then fails
        // to resolve until a clean). Apps are a handful of files; the
        // incremental machinery buys nothing here.
        compileJava.configure { options.isIncremental = false }
        val classesOutputDir = compileJava.flatMap { it.destinationDirectory }

        // Kotlin apps: stage app classes + the shim into one tree, then strip /
        // prune / shake it. The Kotlin plugin is applied *before* this one by
        // picodroid-papk-kotlin, so a plain hasPlugin() check is deterministic.
        val rawClassesInput: Provider<Directory> = if (target.plugins.hasPlugin("org.jetbrains.kotlin.jvm")) {
            val compileKotlin = target.tasks.named("compileKotlin", KotlinCompile::class.java)
            val shimClasses = target.configurations.getByName(PicodroidPapkKotlinPlugin.SHIM_CONFIGURATION)
            val stagedDir = target.layout.buildDirectory.dir("classes-staged")
            val stageClasses = target.tasks.register("stageClasses", Sync::class.java) {
                description = "Stage app + kotlin-shim classes for the strip"
                from(compileKotlin.flatMap { it.destinationDirectory })
                from(compileJava.flatMap { it.destinationDirectory })
                from(shimClasses)
                include("**/*.class")
                into(stagedDir)
            }
            val stripTask = target.tasks.register("stripClassMetadata", StripClassMetadataTask::class.java) {
                dependsOn(stageClasses)
                inputDir.set(stagedDir)
                outputDir.set(target.layout.buildDirectory.dir("classes-stripped"))
                reportFile.set(target.layout.buildDirectory.file("reports/strip-report.txt"))
            }
            stripTask.flatMap { it.outputDir }
        } else {
            classesOutputDir
        }

        // Compile-time API contract (docs/designs/android-parity-roadmap-2026-08.md
        // E3): scan the pre-shrink classes — original names, and for Kotlin
        // apps the staged+stripped shim too — against sdk/api-contract.tsv and
        // the target board's framework_class_excludes. Both knobs are -P
        // properties, never env: a warm daemon's environment is frozen.
        val apiContractMode: Provider<String> =
            target.providers.gradleProperty("picodroid.apiContract").orElse("error")
        val boardName: Provider<String> = target.providers.gradleProperty("picodroid.board")
        val verifyApiContract = target.tasks.register("verifyApiContract", ApiContractTask::class.java) {
            group = "verification"
            description = "Reject java/… references pico-jvm does not serve, and classes the target board excludes"
            classesDir.set(rawClassesInput)
            contractFile.set(repoRoot.resolve("sdk/api-contract.tsv"))
            mode.set(apiContractMode)
            this.boardName.set(boardName)
            boardToml.set(target.layout.file(boardName.map { BoardResolver.boardToml(repoRoot, it) }))
            reportFile.set(target.layout.buildDirectory.file("reports/api-contract.txt"))
            onlyIf { mode.get() != "off" }
        }
        target.tasks.named("check") { dependsOn(verifyApiContract) }

        val packClassesInput = if (frameworkMapVersion != ShrinkMapResolver.UNRELEASED) {
            val mapFile = ShrinkMapResolver.mapFile(repoRoot, frameworkMapVersion)
            val shrinkTask = target.tasks.register("shrinkClasses", ClassShrinkTask::class.java) {
                inputDir.set(rawClassesInput)
                this.mapFile.set(mapFile)
                outputDir.set(target.layout.buildDirectory.dir("classes-shrunk"))
                this.hostTarget.set(hostTarget)
                repoRootPath.set(repoRoot.absolutePath)
            }
            shrinkTask.flatMap { it.outputDir }
        } else {
            rawClassesInput
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

        // Networking examples opt in (`picodroidNetTest { enabled = true }`)
        // to a generated NetTestConfig.java carrying the build-time test-host
        // target — committed default is loopback; the HIL flow or a dev
        // machine overrides it per-invocation (NET-7). Same second-srcDir
        // shape as AssetConstants above.
        val netTest = target.extensions.create("picodroidNetTest", NetTestExtension::class.java)
        netTest.enabled.convention(false)
        val genNetCfg = target.tasks.register(
            "generateNetTestConfig", GenerateNetTestConfigTask::class.java
        ) {
            onlyIf { netTest.enabled.get() }
            packageName.set(manifest.packageName)
            host.set(
                target.providers.gradleProperty("picodroidNetTestHost")
                    .orElse(target.providers.environmentVariable("PICODROID_NET_TEST_HOST"))
                    .orElse("127.0.0.1")
            )
            outputDir.set(target.layout.buildDirectory.dir("generated/picodroid-netcfg"))
        }
        javaExt.sourceSets.getByName("main").java.srcDir(genNetCfg.flatMap { it.outputDir })
        compileJava.configure { dependsOn(genNetCfg) }

        val packPapk = target.tasks.register("packPapk", PapkPackTask::class.java) {
            dependsOn(verifyApiContract)
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
            repoRootPath.set(repoRoot.absolutePath)
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
            repoRootPath.set(repoRoot.absolutePath)
        }
        target.tasks.register("install", RunAppTask::class.java) {
            group = "picodroid"
            description = "Build ${target.name} and push its papk to a connected device"
            dependsOn(assemblePapk)
            mode.set("install")
            appName.set(target.name)
            repoRootPath.set(repoRoot.absolutePath)
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
