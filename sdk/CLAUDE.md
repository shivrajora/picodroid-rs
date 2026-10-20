# sdk/

The Java SDK apps compile against: `java/picodroid/**` (the Android-shaped API), `java/java/**` (the stubs pico-jvm serves), the Kotlin shim, the name tables and the release shrink maps. Rules: `.claude/rules/java-sdk.md`.

- **After the last Java edit:** `./scripts/gen-api-contract.sh` regenerates `class-names.tsv`, `member-names.tsv` and `api-contract.tsv` (javac numbers lambdas by position; the three currency tests in pre-commit fail on a stale table). It ends with `no_original_name_literals`: a Rust string literal equal to a new member name fails it; rename the member.
- **A new class:** a row in `sdk/android-parity.tsv` (status, note), `touch crates/picodroid-core/build.rs`, the website page, the flash check on `testbench_rp2040` (every class is embedded on every board) and, when it cannot fit, `framework_class_excludes` in that board's toml. The shrunk-image gate reads the class as "pending the next release cut" until a cut maps it; cuts are on `main` only, when asked (`CONTRIBUTING.md` › Cutting a release; seven files change).
- **Format:** `./scripts/format_java.sh format` (Google Java Style); the PostToolUse hook does it after every edit and drops unused imports, so an import and its first use go in one edit. Kotlin: `format_kotlin.sh`. `buildSrc/**/*.kt` comments must not contain `/**`-style globs (Kotlin nests block comments).
- **The contract is the fence:** apps compile against the JDK's full `java.*` (`--release 8`), and `verifyApiContract` rejects every reference pico-jvm does not serve; `scripts/`-free check: `./gradlew :examples:<app>:verifyApiContract`. A javap pass over `examples/*/build/classes` shows what the corpus really uses before a builtin is added.
- **Executor-only** concurrency, no `Handler` / `postDelayed`; `Executors.mainScheduledExecutor()` for delays; broadcasts for the clock.
- `shrink-maps/` are immutable once committed; `keep.toml` lists what a map must never rename; `descriptors.tsv` names the descriptors Rust arms use.
