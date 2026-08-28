// SPDX-License-Identifier: GPL-3.0-only
// The ASM reference dump / census / strip tool. Lives outside the fixture so
// its own java/** and kotlin/** references never land in the tables it
// produces, and so Session 2 can lift the pure functions into buildSrc.
plugins {
    id("org.jetbrains.kotlin.jvm")
}

dependencies {
    implementation("org.ow2.asm:asm:9.7")
    implementation("org.ow2.asm:asm-util:9.7")
}

val repoRoot = rootDir.parentFile.parentFile
val outDir = rootDir.resolve("out")

fun registerDump(name: String, label: String, classesDirs: List<Provider<File>>, outSub: String, precondition: (() -> Unit)? = null) =
    tasks.register<JavaExec>(name) {
        group = "survey"
        description = "ASM reference dump of $label -> out/$outSub"
        classpath = sourceSets["main"].runtimeClasspath
        mainClass.set("picodroid.survey.MainKt")
        classesDirs.forEach { inputs.dir(it) }
        outputs.dir(outDir.resolve(outSub))
        doFirst {
            precondition?.invoke()
            val a = mutableListOf("dump", "--label", label, "--out", outDir.resolve(outSub).path)
            classesDirs.forEach { a += listOf("--classes", it.get().path) }
            args = a
        }
    }

fun registerStrip(name: String, label: String, classesDir: Provider<File>, outSub: String) =
    tasks.register<JavaExec>(name) {
        group = "survey"
        description = "ASM strip prototype over $label -> out/$outSub"
        classpath = sourceSets["main"].runtimeClasspath
        mainClass.set("picodroid.survey.MainKt")
        inputs.dir(classesDir)
        outputs.dir(outDir.resolve(outSub))
        doFirst {
            args = listOf("strip", "--label", label, "--classes", classesDir.get().path, "--out", outDir.resolve(outSub).path)
        }
    }

val fixtureClasses = project(":fixture").layout.buildDirectory.dir("classes/kotlin/main").map { it.asFile }
val helloClasses = project(":hello").layout.buildDirectory.dir("classes/kotlin/main").map { it.asFile }
val picoenvmonClasses = provider { repoRoot.resolve("examples/picoenvmon/build/classes/java/main") }

registerDump("dumpRefs", "fixture", listOf(fixtureClasses), "fixture").configure { dependsOn(":fixture:compileKotlin") }
registerDump("dumpHello", "hello", listOf(helloClasses), "hello").configure { dependsOn(":hello:compileKotlin") }
registerDump("dumpJavaBaseline", "picoenvmon-java", listOf(picoenvmonClasses), "picoenvmon-java") {
    val marker = picoenvmonClasses.get().resolve("picoenvmon/EnvApp.class")
    if (!marker.isFile) throw GradleException("missing $marker — run ./gradlew :examples:picoenvmon:compileJava at the repo root")
}

registerStrip("stripFixture", "fixture", fixtureClasses, "strip/fixture").configure { dependsOn(":fixture:compileKotlin") }
registerStrip("stripHello", "hello", helloClasses, "strip/hello").configure { dependsOn(":hello:compileKotlin") }
tasks.register("stripProto") {
    group = "survey"
    description = "Strip prototype over the fixture and hello classes"
    dependsOn("stripFixture", "stripHello")
}

val strippedFixtureClasses = provider { outDir.resolve("strip/fixture/classes") }
registerDump("dumpStripped", "fixture-stripped", listOf(strippedFixtureClasses), "fixture-stripped").configure { dependsOn("stripFixture") }
