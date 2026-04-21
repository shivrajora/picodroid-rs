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
        extensions.configure<JavaPluginExtension> {
            sourceCompatibility = JavaVersion.VERSION_1_8
            targetCompatibility = JavaVersion.VERSION_1_8
        }
        tasks.withType<JavaCompile>().configureEach {
            options.release.set(8)
            options.compilerArgs.addAll(listOf("-Xlint:-options"))
            // Match the javac default (source,lines) instead of Gradle's
            // default (source,lines,vars) so .class bytes match the legacy
            // scripts/build-apk.sh output.
            options.debugOptions.debugLevel = "source,lines"
        }
    }
}
