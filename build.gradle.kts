import net.ltgt.gradle.errorprone.CheckSeverity
import net.ltgt.gradle.errorprone.errorprone

plugins {
    // Adds the `errorprone` config + `options.errorprone {}` DSL. Declared here with
    // `apply false` so the plugin is on the classpath for every Java subproject below,
    // where it's actually applied. On JDK 16+ the plugin forks javac and injects the
    // required --add-exports/--add-opens flags automatically.
    id("net.ltgt.errorprone") version "4.1.0" apply false
}

allprojects {
    repositories {
        mavenCentral()
    }
}

tasks.register("newApp") {
    group = "picodroid"
    description = "Scaffold a new examples/<name>/ app. Usage: ./gradlew newApp -Pname=myapp [-Plang=kotlin]"
    doLast {
        // providers.gradleProperty, not findProperty: `name` is also a built-in
        // Project property (the project's own name), which shadows -Pname.
        val name = providers.gradleProperty("name").orNull
            ?: throw GradleException("missing -Pname=<appname>")
        if (!name.matches(Regex("^[a-z][a-z0-9_]*$"))) {
            throw GradleException("app name must match [a-z][a-z0-9_]* — got '$name'")
        }
        val lang = providers.gradleProperty("lang").orNull ?: "java"
        if (lang != "java" && lang != "kotlin") {
            throw GradleException("-Plang must be java or kotlin — got '$lang'")
        }
        val dir = rootDir.resolve("examples/$name")
        if (dir.exists()) throw GradleException("already exists: $dir")
        val className = name.replaceFirstChar { it.uppercaseChar() }
        if (lang == "kotlin") {
            dir.resolve("kotlin/$name").mkdirs()
            dir.resolve("kotlin/$name/$className.kt").writeText(
                """
                package $name

                import picodroid.app.Application
                import picodroid.util.Log

                class $className : Application() {
                    override fun onCreate() {
                        Log.i("$className", "Hello from $className!")
                    }
                }
                """.trimIndent() + "\n"
            )
        } else {
            dir.resolve("java/$name").mkdirs()
            dir.resolve("java/$name/$className.java").writeText(
                """
                package $name;

                import picodroid.app.Application;
                import picodroid.util.Log;

                public class $className extends Application {
                  @Override
                  public void onCreate() {
                    Log.i("$className", "Hello from $className!");
                  }
                }
                """.trimIndent() + "\n"
            )
        }
        dir.resolve("PicodroidManifest.xml").writeText(
            """
            <?xml version="1.0" encoding="utf-8"?>
            <manifest package="$name" version="1.0"
                      xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                      xsi:noNamespaceSchemaLocation="../../schema/PicodroidManifest.xsd">
                <application application="$name/$className" />
            </manifest>
            """.trimIndent() + "\n"
        )
        val pluginId = if (lang == "kotlin") "picodroid-papk-kotlin" else "picodroid-papk"
        dir.resolve("build.gradle.kts").writeText(
            """
            plugins {
                id("$pluginId")
            }
            """.trimIndent() + "\n"
        )
        println("==> Created examples/$name/ — build with: ./scripts/build-apk.sh --app $name")
    }
}

subprojects {
    plugins.withType<JavaPlugin> {
        apply(plugin = "net.ltgt.errorprone")
        extensions.configure<JavaPluginExtension> {
            sourceCompatibility = JavaVersion.VERSION_1_8
            targetCompatibility = JavaVersion.VERSION_1_8
        }
        dependencies {
            "errorprone"("com.google.errorprone:error_prone_core:2.36.0")
        }
        // One-shot sweep that applies the auto-fixable checks in place (insert missing
        // @Override, strip unused imports):
        //   ./gradlew compileJava -Pep.patch
        // Leave it off for normal builds, where the checks below are hard errors.
        val epPatch = project.hasProperty("ep.patch")
        tasks.withType<JavaCompile>().configureEach {
            options.release.set(8)
            options.compilerArgs.addAll(listOf("-Xlint:-options"))
            // Match the javac default (source,lines) instead of Gradle's
            // default (source,lines,vars) so .class bytes match the legacy
            // scripts/build-apk.sh output.
            options.debugOptions.debugLevel = "source,lines"
            options.errorprone {
                if (epPatch) {
                    // Auto-fix sweep — stay surgical (only the patchable checks run) so
                    // nothing else can fail the compile before the patches are written.
                    disableAllChecks.set(true)
                    check("MissingOverride", CheckSeverity.WARN)
                    check("RemoveUnusedImports", CheckSeverity.WARN)
                    errorproneArgs.addAll(
                        "-XepPatchChecks:MissingOverride,RemoveUnusedImports",
                        "-XepPatchLocation:IN_PLACE",
                    )
                } else {
                    // Error Prone's default ERROR tier (curated, ~zero-false-positive
                    // real-bug detectors: FormatString, EqualsHashCode, ReturnValueIgnored,
                    // ArrayToString, ComparisonOutOfRange, BoxedPrimitiveEquality, …) now
                    // fails the build. Default WARNING checks still print but don't fail.
                    //
                    // Subset-aware: picodroid's String has no toUpperCase(Locale) overload,
                    // so this default warning would be an unfixable false positive.
                    check("StringCaseLocaleUsage", CheckSeverity.OFF)

                    // Generated sources (AssetConstants, NetTestConfig, the
                    // @Inject processor's *_Factory / *_MembersInjector) are
                    // not hand-maintained and not format-gated; keep the
                    // curated ERROR checks off them.
                    excludedPaths.set(".*/build/generated/.*")

                    // Curated readability/safety checks promoted to build-failing. @Override
                    // and unused imports are source-only (no .class byte change) and
                    // auto-fixable via the -Pep.patch sweep above.
                    check("MissingOverride", CheckSeverity.ERROR)
                    check("ReferenceEquality", CheckSeverity.ERROR) // no == on object refs
                    check("RemoveUnusedImports", CheckSeverity.ERROR)
                    check("FallThrough", CheckSeverity.ERROR) // switch fall-through
                    check("OperatorPrecedence", CheckSeverity.ERROR) // ambiguous & / == / ?:
                    check("UnusedVariable", CheckSeverity.ERROR) // dead locals/fields
                }
            }
        }
    }
}
