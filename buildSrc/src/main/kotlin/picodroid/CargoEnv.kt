package picodroid

/**
 * When the Gradle daemon is spawned from inside a `cargo build` (our
 * firmware build.rs calls `./gradlew :sdk:compileJava`), it inherits Cargo's
 * per-crate environment — `CARGO_MANIFEST_DIR`, `OUT_DIR`, `RUSTC_WRAPPER`,
 * `TARGET`, and so on. If a later Gradle task spawns its own `cargo run`
 * (e.g. to invoke `tools/class-shrink`) the nested Cargo picks up those
 * stale vars and fails with cryptic "could not execute process" errors.
 *
 * Strip the problematic prefixes before spawning. Leaves `CARGO_HOME`
 * alone since that's the legitimate Cargo config dir, not per-build state.
 */
object CargoEnv {
    private val KEEP_PREFIXES = setOf("CARGO_HOME", "CARGO_TARGET_DIR")

    fun sanitize(pb: ProcessBuilder) {
        val env = pb.environment()
        val doomed = env.keys.filter { key ->
            (key.startsWith("CARGO_") || key.startsWith("RUSTC_") || key == "OUT_DIR" ||
                key == "TARGET" || key == "HOST" || key == "NUM_JOBS" ||
                key == "OPT_LEVEL" || key == "PROFILE" || key == "DEBUG")
                && key !in KEEP_PREFIXES
        }
        doomed.forEach { env.remove(it) }
    }
}
