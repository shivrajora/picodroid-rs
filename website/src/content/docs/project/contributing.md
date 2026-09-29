---
title: "Contributing to Picodroid"
description: "How to set up the toolchain, run pre-commit, and contribute to Picodroid."
---

## Getting Set Up

See [Build & flash](/get-started/build/) for full prerequisites (Rust toolchain, ARM cross-compiler, JDK 21, probe-rs). The Java formatter is a Java 21 jar; when `java` on your `PATH` is older, point `JAVA_HOME` at a JDK 21 and the formatter scripts use that one.

Quick version:

```bash
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

## After Every Change

Two checks, both cheap. CI and the nightlies are the regression gates, not your machine.

1. **Sim smoke.** After a change under `crates/`, `platforms/`, `sdk/` or `system-apps/`:

   ```bash
   ./scripts/sim.sh --app helloworld
   ```

   Confirm `[HelloWorld] Hello, World!` appears. Docs, example-app and script-only edits need
   no smoke; every other app runs in CI's sim smoke or in the 3 AM sim nightly.

2. **Pre-commit.** `./scripts/pre-commit` must end with `==> All checks passed.` See
   [Pre-commit Hook](#pre-commit-hook).

Skip both during intermediate debugging steps, and run them once the fix is in. When the bug is
about memory (heap growth, churn, OOM, corruption) or scheduling (a task hogging a core,
starving a peer, a spin that runs long), the opt-in monitors are one flag away:
`./scripts/sim.sh --app <app> --mem-diag` and `--sched-diag`
([`docs/memory-diagnostics.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/memory-diagnostics.md),
[`docs/scheduling-diagnostics.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/scheduling-diagnostics.md)).

## Running Tests

Use the test script — bare `cargo test` fails because there is no default cargo target and the firmware crate is bare-metal ARM:

```bash
./scripts/test.sh
```

It runs the workspace on the host twice, with the shrinker off and on, and takes no arguments. CI runs it on every push, so it is not part of the two checks above; run it when you have changed something its tests pin (a native dispatch arm, the class tables, a wire format).

## Pre-commit Hook

Install it after cloning:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

It runs in two tiers, both of which fan their stages out across parallel lanes:

```bash
./scripts/pre-commit          # fast (default, and what the hook runs)
./scripts/pre-commit --full   # the release-cut gate
```

**`--fast`** takes seconds and builds nothing: the source-tree guards that
exist nowhere else (shadow twins across `platforms/rp/src` and
`crates/picodroid-core/src`, `family-rp` cfg-gate hygiene, `apply_jvm_env`)
plus whichever of `cargo fmt`, the Java and Kotlin formatters and markdown lint
the changed files implicate. A `scripts/` change adds the `hil-tests.conf`
drift check and the device-lock test.

**`--full`** runs the legs neither CI nor the nightlies cover: the
`legacy-handle-cast` clippy leg, the opt-in `mem-diag` / `sched-diag`
firmware builds, `pico_enviro_mon_w` clippy, the shrunk-image name check and
the binary-size ratchet on both boards. A few minutes; run it before cutting a
release.

Everything else is CI's job, so pushing does not wait for `--full`.
`.github/workflows/ci_checks.yml` runs every board's clippy, both boards in
debug and release, `test.sh` in both shrink modes, every example APK and its
API contract, both formatters, and a sim smoke in four parallel shards covering
all three langsuites; its Building job also links one WiFi board in debug
(`pico_display2_w`), the largest image of the fleet, and its Linting job runs on
Java 21. `ci_light.yml` runs the same source guards, the markdown lint and
the docs-site link check, docs-only commits included. The 3 AM `sim-run.sh` nightly runs the whole
`hil-tests.conf` matrix in both shrink modes — the `qa_*` apps, the diagnostics
soaks and the binary-size ratchet — and the 4 AM `hil-fleet.sh` runs it on
every bench board. CI takes about 20 minutes; after a push, `gh run list --limit 3`
shows it. Nightly results arrive by email and under `build/sim/results/` and
`build/hil/results/`.

Useful flags:

| Flag | Effect |
| --- | --- |
| `--list` | Print the stages that would run, grouped by lane, and exit. |
| `--serial` | One lane at a time, streaming to stdout. Use it to debug a failure. |
| `--since <ref>` | Scope against `<ref>` instead of the index or working tree. |
| `--clean` | Delete the per-lane build directories. |

Each cargo lane gets its own `CARGO_TARGET_DIR` (`target/` for host,
`target/lane-thumbv6m/` and `target/lane-thumbv8m/` for the two ARM triples)
because cargo serializes concurrent invocations that share one build directory.
The first `--full` run pays a cold build for the two ARM directories; `--clean` removes them, and so does `cargo clean`, which now
covers the lanes as well. Per-run logs land in `build/pre-commit/`.

## Sharing the Bench

One or more boards, each with its own debug probe, and often more than one
session wanting them — a second terminal, an agent working in a worktree,
the nightly HIL run. Every script that touches a board (`flash.sh`,
`power-cycle.sh`, `pdb.sh`, `parity-bench.sh --hil`, `hil-run.sh`) takes a
lease on that one board through `scripts/device-lock.sh` first. Name the
board with `--board NAME` (or `--slot NAME`); with neither, the script uses
the board your session already holds, or the only one on the bench, or
stops and lists the slots. If the board is free the script acquires it for
your session and keeps it until you release, so a flash followed by a few
`pdb` calls needs no ceremony; if someone else holds it the script exits
with code 75, names the holder, and tells you how to wait. The other boards
stay free.

```bash
./scripts/device-lock.sh status                     # every slot: holder, since when, queue
./scripts/device-lock.sh acquire --board X --wait   # queue (FIFO) until that board is yours
./scripts/device-lock.sh release                    # everything you hold; also kills a lingering probe-rs
./scripts/device-lock.sh break --slot X --force     # evict a holder who is really gone
```

The bench is described by `~/.config/picodroid/fleet.conf`, one line per
board slot: the probe's USB serial, the USB position of the board's own
port (the pdb device has no serial, so its physical port is its identity),
and the firmware boards that hardware accepts. `scripts/fleet.conf.example`
documents the format, `./scripts/fleet.sh discover` prints the probes and
boards it can see with their positions, and `./scripts/fleet.sh check`
validates the file. Without the file every script assumes a single board,
as before. `./scripts/hil-fleet.sh` runs the nightly on every slot at once,
each runner with its own build directory, results and email; a runner waits
up to an hour for its board, then records a SKIP.

A HIL row's window is its timeout in `hil-tests.conf` plus a flash budget of
80 seconds, because `probe-rs run` flashes and captures the log in one
invocation and the RP2350 images take 53–72 s to flash. The RP2040's rows get
twice the timeout. A row with an empty log and a `MISSING` verdict usually
means the flash took the whole window.

WiFi boards (`testbench_rp2350w`, `pico_enviro_mon_w`, `pico_display2_w`,
`pico_touch_kit`) take `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` at build
time. Keep local credentials in the gitignored `.wifi-creds.env` at the repo
root: `hil-run.sh` reads that file for the `net` rows of `hil-tests.conf` and
skips them when it is missing. A board can also be given its network from
Settings → Wi-Fi; build-time credentials take precedence.

A lease dies with the process that took it (your shell, or the agent
session), so a closed window never wedges a board. An unattended run that
must outlive its launcher pins the lease instead:
`PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire --board X --pin`
before the flash, `release` at teardown. Never `pkill -f probe-rs` to free a
probe — the pattern matches any shell whose command line mentions it, your
own included; `release` kills only the probe-rs on your board's probe.
`PICODROID_DEVICE_LOCK=0` bypasses the check, for emergencies only.

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

1. Add the native implementation in `crates/picodroid-core/src/native_handler/` under the appropriate module
2. Register it: a new native class goes in `PICODROID_NATIVE_CLASSES` (`crates/picodroid-core/src/native_handler/class_registry.rs`), and every dispatch arm needs a matching `(class, method, descriptor)` row in `crates/picodroid-core/src/native_handler/method_tables.rs` — tests cross-check both. Both names go through the generated `shrink_names` consts, never a string literal: `(c::picodroid_pio_Gpio, m::setValue) =>` (each const's value is the map's shrunk spelling under `--shrink` and the original otherwise; a literal would silently stop matching under `--shrink` and put the original name back into flash — `no_original_name_literals` refuses it). A new SDK class or method first needs a row in `sdk/class-names.tsv` / `sdk/member-names.tsv`: run `scripts/gen-api-contract.sh`. Descriptors that name a class come from `sdk/descriptors.tsv` (`d::String__V`); add a row by hand. Arms on `java/**` owners (e.g. `System.currentTimeMillis`) are the same, with `c::java_lang_System`. See [Shrinker](/reference/shrinker/) for details.
3. If adding a new class to `BuiltinHandler`, also register it in `class_name_to_static_in` in `crates/jvm/src/interpreter/helpers.rs` — otherwise virtual dispatch will silently break
4. Add the Java API stub in `sdk/java/picodroid/`. The class will be picked up automatically by the next release cut; between releases its name stays un-shrunk. Every SDK class is embedded, with its link table, in every board's firmware: check the flash it costs on `testbench_rp2040`, and if that board cannot carry it, add the class to its `framework_class_excludes`.
5. Update the relevant [API reference](/api/) page (e.g. [Peripherals](/api/peripherals/) for a new PIO method, [Graphics & UI](/api/ui/) for a new widget) with the new API surface

> **Docs are mirrored.** This page is a copy of the repository's root `CONTRIBUTING.md` — edit both together so they don't drift. Likewise, if you change a board memory value (`board.toml`, `FreeRTOSConfig.h`, or the MCU `.toml`s), re-check [Limits & memory budgets](/reference/limits/), which quotes those numbers.

## Two Source Trees

Family-neutral framework code lives in `crates/picodroid-core/` (JVM natives, lifecycle,
graphics, networking, the simulator's HAL); family-specific code lives in `platforms/rp/`.
Never create a file at the same relative path in both `src` trees: the pre-commit shadow-twin
guard rejects it. When moving code between them, move it; don't copy. The
[architecture page](/project/architecture/) has the module map and the rules that go with it.

Apps and examples import the SDK as `picodroid.*` (`import picodroid.view.View;`). The API is
named to mirror `android.*` method for method, but there is no `android.*` alias layer, so an
`android.*` import neither compiles nor loads.

## Cutting a New Release

Run `./scripts/pre-commit --full` first: it is the release-cut gate.

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
  --contract sdk/api-contract.tsv --reserve sdk/kotlin-shim/build/classes/java/main \
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
2. Test your changes with the simulator (`./scripts/sim.sh --app helloworld`) and on hardware if possible; CI runs the full matrix on your push
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
