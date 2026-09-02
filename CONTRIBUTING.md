# Contributing to Picodroid

## Getting Set Up

See [website/src/content/docs/get-started/build.md](website/src/content/docs/get-started/build.md) for full prerequisites (Rust toolchain, ARM cross-compiler, JDK 11+, probe-rs).

Quick version:

```bash
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

## Running Tests

Always use the test script — bare `cargo test` fails because the default target is bare-metal ARM:

```bash
./scripts/test.sh
```

## Pre-commit Hook

Install it after cloning:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

It runs in two tiers, both of which fan their stages out across parallel lanes:

```bash
./scripts/pre-commit          # fast (default, and what the hook runs)
./scripts/pre-commit --full   # everything — run before you push
```

**`--fast`** is scoped to what actually changed and trimmed to the checks CI
does not already run. A docs-only commit gets markdown lint and the guards; a
one-file Rust change adds `cargo fmt`, sim + RP2040 clippy, the RP2040 debug
flash gate and the size ratchet. Editing anything under `scripts/` promotes the
run to `--full`, since a script change can invalidate any lane's assumptions.

**`--full`** runs every check unscoped: all five board clippy legs, the staged
`handle-table-32` and opt-in `mem-diag` legs, every firmware build, the test
suite in both shrink modes, the Java and Kotlin conformance suites in the
simulator, and the size ratchet on both boards.

What the fast tier is allowed to skip is not a guess. `.github/workflows/ci_checks.yml`
already runs the board clippy legs, both boards in debug and release, `test.sh`,
every example APK, both formatters, and a 14-app sim smoke covering all three
langsuites. The checks that exist *only* locally — the shadow-twin and
cfg-hygiene guards, `hil-tests.conf` drift, `apply_jvm_env`, markdown lint, and
the binary-size ratchet — run at every tier.

Useful flags:

| Flag | Effect |
| --- | --- |
| `--list` | Print the stages that would run, grouped by lane, and exit. |
| `--serial` | One lane at a time, streaming to stdout. Use it to debug a failure. |
| `--since <ref>` | Scope against `<ref>` instead of the index or working tree. |
| `--clean` | Delete the per-lane build directories. |

Each cargo lane gets its own `CARGO_TARGET_DIR` (`target/` for host,
`target-thumbv6m/` and `target-thumbv8m/` for the two ARM triples) because cargo
serializes concurrent invocations that share one build directory. The first
`--full` run after checkout therefore pays a cold build for the two ARM
directories; `--clean` removes them. Per-run logs land in `build/pre-commit/`.

## Code Style

### Rust

- Format with `cargo fmt` before committing
- Clippy must pass with `--deny=warnings` on all targets

### Java

- All Java sources follow [Google Java Style](https://google.github.io/styleguide/javaguide.html)
- Reformat in-place: `./scripts/format_java.sh format`
- Check without modifying: `./scripts/format_java.sh check`

## Adding a New Example App

1. Create the directory structure:

```text
examples/myapp/
  java/myapp/MyApp.java
  PicodroidManifest.xml
```

2. Write your Java source as an `Application` subclass with an `onCreate()` entry point:

```java
package myapp;

import picodroid.app.Application;
import picodroid.util.Log;

public class MyApp extends Application {
    public void onCreate() {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

3. Create `PicodroidManifest.xml` (note: the attribute is `application`, not `main-class`):

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="myapp" version="1.0">
    <application application="myapp/MyApp" />
</manifest>
```

4. Build and test:

```bash
./scripts/build.sh --app myapp
./scripts/sim.sh --app myapp        # test on host first
./scripts/flash.sh --app myapp      # flash to hardware
```

5. Add your app to [website/src/content/docs/examples.md](website/src/content/docs/examples.md) in the appropriate category.

`./gradlew newApp -Pname=myapp` scaffolds steps 1–3 for you. See [website/src/content/docs/reference/manifest.md](website/src/content/docs/reference/manifest.md) for the full manifest schema and entry-point styles, and [website/src/content/docs/get-started/first-app.md](website/src/content/docs/get-started/first-app.md) for supported language features and the full Java API.

> **Docs are mirrored.** `website/src/content/docs/project/contributing.md` is a copy of this file — edit both together. If you change a board memory value (`board.toml`, `FreeRTOSConfig.h`, or the MCU `.toml`s), also re-check [website/src/content/docs/reference/limits.md](website/src/content/docs/reference/limits.md), which quotes those numbers.

## Adding a New Native Java Method

When adding a new native method that the JVM dispatches to Rust:

1. Add the native implementation in `picodroid-core/src/native_handler/` under the appropriate module
2. Register it: a new native class goes in `PICODROID_NATIVE_CLASSES` (`picodroid-core/src/native_handler/class_registry.rs`), and every dispatch arm needs a matching `(class, method, descriptor)` row in `picodroid-core/src/native_handler/method_tables.rs` — tests cross-check both. Use the **original** internal class name in the match arm (e.g. `"picodroid/pio/Gpio"`) — the dispatcher calls `shrink_names::unshrink_class` at entry so names stay readable in source regardless of the active shrink map. The **method** name goes through the generated `shrink_names::m` consts, never a string literal: `("picodroid/pio/Gpio", m::setValue) =>` (the const's value is the map's shrunk spelling under `--shrink`; a literal would silently stop matching there — `no_sdk_method_literals_in_dispatch` refuses it). A new SDK method first needs a row in `sdk/member-names.tsv`: run `scripts/gen-api-contract.sh`. Arms on `java/**` owners (e.g. `System.currentTimeMillis`) are the same: pico-jvm reverse-translates its `b/` namespace at the class-file boundary, so dispatch never sees a shrunk `java/**` name. See [website/src/content/docs/reference/shrinker.md](website/src/content/docs/reference/shrinker.md) for details.
3. If adding a new class to `BuiltinHandler`, also register it in `class_name_to_static_in` in `jvm/src/interpreter/helpers.rs` — otherwise virtual dispatch will silently break
4. Add the Java API stub in `sdk/java/picodroid/`. The class will be picked up automatically by the next release cut; between releases its name stays un-shrunk.
5. Update the relevant `website/src/content/docs/api/*.md` (e.g. `api/peripherals.md` for a new PIO method, `api/ui.md` for a new widget) with the new API surface

## Cutting a New Release

Shrink maps are tied 1:1 to picodroid package versions and are immutable
once committed. Shrinking itself is **off by default** (opt-in per build
via `--shrink`), but every release ships a committed map so
`--shrink`-enabled builds have something to resolve against. When you
bump the `version` in `platforms/rp/Cargo.toml`, cut a fresh map in the
same commit:

```bash
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

./gradlew :kotlin-shim:compileJava -q
cargo run -p class-shrink -- cut-release --members \
  --keep-contract sdk/api-contract.tsv --reserve kotlin-shim/build/classes/java/main \
  --version <new> \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --extra-names sdk/api-contract.tsv \
  --base sdk/shrink-maps/v<previous>.toml \
  --out  sdk/shrink-maps/v<new>.toml
```

`--base` copies the previous map verbatim — existing entries never get
renamed. `--extra-names` adds the `java/**` names the framework never
references itself, so apps' `RuntimeException` / `Iterator` / … shrink
too. See [website/src/content/docs/reference/shrinker.md](website/src/content/docs/reference/shrinker.md) for the full design.

## Submitting Changes

1. Make sure `./scripts/pre-commit` passes with `==> All checks passed.`
2. Test your changes with the simulator (`./scripts/sim.sh`) and on hardware if possible
3. Keep commits focused — one logical change per commit
4. Open a pull request with a clear description of what changed and why

## License

picodroid-rs is dual-licensed: it is available to the public under the
GPL-3.0-only license (see [LICENSE](LICENSE)), and separately under a
proprietary commercial license for customers who need to distribute
closed-source derivatives. See [LICENSING.md](LICENSING.md).

To preserve the project's ability to offer the commercial license, every
contribution must be made under the terms of [CLA.md](CLA.md). By opening a
pull request, you grant the project maintainer a perpetual, worldwide,
non-exclusive, irrevocable, royalty-free license to reproduce, prepare
derivative works of, and distribute your contribution as part of picodroid-rs
under the GPL-3.0-only license **and** under any other license the maintainer
chooses (including the proprietary commercial license).

You retain copyright in your contribution and may continue to use, license,
or relicense your own contribution however you wish. The grant above is
non-exclusive — it does not transfer ownership and does not prevent you from
distributing your standalone contribution under any other terms you choose.
