// SPDX-License-Identifier: GPL-3.0-only
// The hello-kotlin-on-sim milestone: one zero-stdlib Kotlin Application,
// hand-packed with tools/papk-pack (mirrors buildSrc PapkPackTask.kt) and run
// through `scripts/sim.sh --apk` by ../hello-sim.sh.
plugins {
    id("org.jetbrains.kotlin.jvm")
}

val repoRoot = rootDir.parentFile.parentFile
val sdkClasses = repoRoot.resolve("sdk/build/classes/java/main")
val outDir = rootDir.resolve("out")

dependencies {
    compileOnly(files(sdkClasses))
}

tasks.named("compileKotlin") {
    doFirst {
        if (!sdkClasses.resolve("picodroid/util/Log.class").isFile) {
            throw GradleException(
                "SDK classes missing at $sdkClasses — run ./gradlew :sdk:compileJava at the repo root"
            )
        }
    }
}

// `rustc -vV` host triple, exactly as scripts/lib.sh host_target() and
// PapkPackTask.kt compute it, so the papk-pack binary shares target/<host>/.
val hostTarget = providers.exec { commandLine("rustc", "-vV") }
    .standardOutput.asText.map { text ->
        text.lineSequence().first { it.startsWith("host:") }.substringAfter("host:").trim()
    }

fun registerPack(name: String, classesDir: Provider<File>, papkName: String) =
    tasks.register<Exec>(name) {
        group = "survey"
        description = "Pack $papkName with tools/papk-pack"
        workingDir = repoRoot
        inputs.dir(classesDir)
        inputs.property("hostTarget", hostTarget)
        outputs.file(outDir.resolve(papkName))
        doFirst {
            outDir.mkdirs()
            // Same hygiene as buildSrc CargoEnv.sanitize: a daemon spawned from
            // inside a `cargo build` carries per-crate env that breaks a nested cargo.
            environment.keys.filter { k ->
                (k.startsWith("CARGO_") && k != "CARGO_HOME" && k != "CARGO_TARGET_DIR") ||
                    k.startsWith("RUSTC_") || k in setOf("OUT_DIR", "TARGET", "HOST", "NUM_JOBS", "OPT_LEVEL", "PROFILE", "DEBUG")
            }.forEach { environment.remove(it) }
            commandLine(
                "cargo", "run", "--quiet",
                "--target", hostTarget.get(),
                "--manifest-path", repoRoot.resolve("tools/papk-pack/Cargo.toml").path,
                "--",
                "--application", "hellokt/HelloKt",
                "--package-name", "hellokt",
                "--version", "1.0",
                "--framework-map-version", "0.0.0",
                "--classes-dir", classesDir.get().path,
                "--output", outDir.resolve(papkName).path,
            )
        }
    }

registerPack(
    "helloPapk",
    layout.buildDirectory.dir("classes/kotlin/main").map { it.asFile },
    "hellokt.papk",
).configure { dependsOn("compileKotlin") }

// The ASM-stripped hello class (out/strip/hello/classes, written by :dump:stripHello):
// proves ClassWriter(0) output without StackMapTable loads and runs on pico-jvm.
registerPack(
    "helloPapkStripped",
    provider { outDir.resolve("strip/hello/classes") },
    "hellokt-stripped.papk",
).configure { dependsOn(":dump:stripHello") }
