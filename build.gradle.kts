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
    description = "Scaffold a new examples/<name>/ app. Usage: ./gradlew newApp -Pname=myapp"
    doLast {
        val name = project.findProperty("name") as? String
            ?: throw GradleException("missing -Pname=<appname>")
        if (!name.matches(Regex("^[a-z][a-z0-9_]*$"))) {
            throw GradleException("app name must match [a-z][a-z0-9_]* — got '$name'")
        }
        val dir = rootDir.resolve("examples/$name")
        if (dir.exists()) throw GradleException("already exists: $dir")
        val className = name.replaceFirstChar { it.uppercaseChar() }
        dir.resolve("java/$name").mkdirs()
        dir.resolve("java/$name/$className.java").writeText(
            """
            package $name;

            import picodroid.app.Application;
            import picodroid.util.Log;

            public class $className extends Application {
              public void onCreate() {
                Log.i("$className", "Hello from $className!");
              }
            }
            """.trimIndent() + "\n"
        )
        dir.resolve("PicodroidManifest.xml").writeText(
            """
            <?xml version="1.0" encoding="utf-8"?>
            <manifest package="$name" version="1.0">
                <application application="$name/$className" />
            </manifest>
            """.trimIndent() + "\n"
        )
        dir.resolve("build.gradle.kts").writeText(
            """
            plugins {
                id("picodroid-papk")
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
        // One-shot sweep that inserts every missing @Override in place:
        //   ./gradlew compileJava -Pep.patch
        // Leave it off for normal builds, where MissingOverride is a hard error.
        val epPatch = project.hasProperty("ep.patch")
        tasks.withType<JavaCompile>().configureEach {
            options.release.set(8)
            options.compilerArgs.addAll(listOf("-Xlint:-options"))
            // Match the javac default (source,lines) instead of Gradle's
            // default (source,lines,vars) so .class bytes match the legacy
            // scripts/build-apk.sh output.
            options.debugOptions.debugLevel = "source,lines"
            // Enforce only @Override; every other Error Prone check is left off so
            // unrelated patterns can't break the build. @Override is source-retention,
            // so this never changes the emitted .class bytes.
            options.errorprone {
                disableAllChecks.set(true)
                if (epPatch) {
                    check("MissingOverride", CheckSeverity.WARN)
                    errorproneArgs.addAll(
                        "-XepPatchChecks:MissingOverride",
                        "-XepPatchLocation:IN_PLACE",
                    )
                } else {
                    check("MissingOverride", CheckSeverity.ERROR)
                }
            }
        }
    }
}
