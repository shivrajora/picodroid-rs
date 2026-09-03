---
title: "Contributing to Picodroid"
description: "How to set up the toolchain, run pre-commit, and contribute to Picodroid."
---

## Getting Set Up

See [Build & flash](/get-started/build/) for full prerequisites (Rust toolchain, ARM cross-compiler, JDK 11+, probe-rs).

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

## Sharing One Dev Board

One probe, one board, and often more than one session wanting it — a second
terminal, an agent working in a worktree, the nightly HIL run. Every script
that touches the board (`flash.sh`, `power-cycle.sh`, `pdb.sh`,
`parity-bench.sh --hil`, `hil-run.sh`) takes a machine-wide lease through
`scripts/device-lock.sh` first. If the board is free the script acquires it
for your session and keeps it until you release, so a flash followed by a few
`pdb` calls needs no ceremony; if someone else holds it the script exits with
code 75, names the holder, and tells you how to wait.

```bash
./scripts/device-lock.sh status           # who holds it, since when, who is queued
./scripts/device-lock.sh acquire --wait   # queue (FIFO) until the board is yours
./scripts/device-lock.sh release          # when you are done; also kills a lingering probe-rs
./scripts/device-lock.sh break --force    # evict a holder who is really gone
```

A lease dies with the process that took it (your shell, or the agent
session), so a closed window never wedges the board. An unattended run that
must outlive its launcher pins the lease instead:
`PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire --pin` before
the flash, `release` at teardown. Never `pkill -f probe-rs` to free the probe
— the pattern matches any shell whose command line mentions it, your own
included; `release` kills the right process by name. `PICODROID_DEVICE_LOCK=0`
bypasses the check, for emergencies only.

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

```
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

5. Add your app to the [Examples](/examples/) catalog in the appropriate category.

See [Your first app](/get-started/first-app/) for supported language features and the full Java API.

## Adding a New Native Java Method

When adding a new native method that the JVM dispatches to Rust:

1. Add the native implementation in `picodroid-core/src/native_handler/` under the appropriate module
2. Register it: a new native class goes in `PICODROID_NATIVE_CLASSES` (`picodroid-core/src/native_handler/class_registry.rs`), and every dispatch arm needs a matching `(class, method, descriptor)` row in `picodroid-core/src/native_handler/method_tables.rs` — tests cross-check both. Both names go through the generated `shrink_names` consts, never a string literal: `(c::picodroid_pio_Gpio, m::setValue) =>` (each const's value is the map's shrunk spelling under `--shrink` and the original otherwise; a literal would silently stop matching under `--shrink` and put the original name back into flash — `no_original_name_literals` refuses it). A new SDK class or method first needs a row in `sdk/class-names.tsv` / `sdk/member-names.tsv`: run `scripts/gen-api-contract.sh`. Descriptors that name a class come from `sdk/descriptors.tsv` (`d::String__V`); add a row by hand. Arms on `java/**` owners (e.g. `System.currentTimeMillis`) are the same, with `c::java_lang_System`. See [Shrinker](/reference/shrinker/) for details.
3. If adding a new class to `BuiltinHandler`, also register it in `class_name_to_static_in` in `jvm/src/interpreter/helpers.rs` — otherwise virtual dispatch will silently break
4. Add the Java API stub in `sdk/java/picodroid/`. The class will be picked up automatically by the next release cut; between releases its name stays un-shrunk.
5. Update the relevant [API reference](/api/) page (e.g. [Peripherals](/api/peripherals/) for a new PIO method, [Graphics & UI](/api/ui/) for a new widget) with the new API surface

> **Docs are mirrored.** This page is a copy of the repository's root `CONTRIBUTING.md` — edit both together so they don't drift. Likewise, if you change a board memory value (`board.toml`, `FreeRTOSConfig.h`, or the MCU `.toml`s), re-check [Limits & memory budgets](/reference/limits/), which quotes those numbers.

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
  --contract sdk/api-contract.tsv --reserve kotlin-shim/build/classes/java/main \
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
too; `--contract` maps the members the runtime serves on those classes.
If a new SDK class or member is used from Rust before the next cut, run
`scripts/gen-api-contract.sh` so `sdk/class-names.tsv` /
`sdk/member-names.tsv` — the sources of the generated `c::` / `m::`
constants — know about it. See [Shrinker](/reference/shrinker/) for the full design.

## Submitting Changes

1. Make sure `./scripts/pre-commit` passes with `==> All checks passed.`
2. Test your changes with the simulator (`./scripts/sim.sh`) and on hardware if possible
3. Keep commits focused — one logical change per commit
4. Open a pull request with a clear description of what changed and why

## License

picodroid-rs is dual-licensed: it is available to the public under the
GPL-3.0-only license (see [LICENSE](https://github.com/shivrajora/picodroid-rs/blob/main/LICENSE)), and separately under a
proprietary commercial license for customers who need to distribute
closed-source derivatives. See [Licensing](/project/licensing/).

To preserve the project's ability to offer the commercial license, every
contribution must be made under the terms of [CLA](/project/cla/). By opening a
pull request, you grant the project maintainer a perpetual, worldwide,
non-exclusive, irrevocable, royalty-free license to reproduce, prepare
derivative works of, and distribute your contribution as part of picodroid-rs
under the GPL-3.0-only license **and** under any other license the maintainer
chooses (including the proprietary commercial license).

You retain copyright in your contribution and may continue to use, license,
or relicense your own contribution however you wish. The grant above is
non-exclusive — it does not transfer ownership and does not prevent you from
distributing your standalone contribution under any other terms you choose.
