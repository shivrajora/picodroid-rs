---
title: "Contributing to Picodroid"
description: "How to set up the toolchain, run pre-commit, and contribute to Picodroid."
---

## Getting Set Up

See [docs/getting-started.md](/get-started/build/) for full prerequisites (Rust toolchain, ARM cross-compiler, JDK 11+, probe-rs).

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

The pre-commit hook runs automatically on `git commit` and checks:

1. Java formatting (`google-java-format`)
2. Rust formatting (`cargo fmt`)
3. Clippy (RP2040, RP2350, and simulator targets)
4. Embedded firmware build
5. All tests

Install it after cloning:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

You can also run it manually at any time:

```bash
./scripts/pre-commit
```

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

5. Add your app to `docs/examples.md` in the appropriate category.

See [docs/writing-apps.md](/get-started/first-app/) for supported language features and the full Java API.

## Adding a New Native Java Method

When adding a new native method that the JVM dispatches to Rust:

1. Add the native implementation in `src/system/` under the appropriate module
2. Register the method in the `NativeMethodHandler` dispatch in `src/system/`. Use the **original** internal class name in the match arm (e.g. `"picodroid/pio/Gpio"`) — the dispatcher calls `shrink_names::unshrink_class` at entry so names stay readable in source regardless of the active shrink map. See [docs/shrinker.md](/reference/shrinker/) for details.
3. If adding a new class to `BuiltinHandler`, also register it in `class_name_to_static_in` in `jvm/src/helpers.rs` — otherwise virtual dispatch will silently break
4. Add the Java API stub in `sdk/java/picodroid/`. The class will be picked up automatically by the next release cut; between releases its name stays un-shrunk.
5. Update the relevant `docs/api/*.md` (e.g. `api/peripherals.md` for a new PIO method, `api/ui.md` for a new widget) with the new API surface

## Cutting a New Release

Shrink maps are tied 1:1 to picodroid package versions and are immutable
once committed. Shrinking itself is **off by default** (opt-in per build
via `--shrink`), but every release ships a committed map so
`--shrink`-enabled builds have something to resolve against. When you
bump the `version` in the root `Cargo.toml`, cut a fresh map in the
same commit:

```bash
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

cargo run -p class-shrink -- cut-release \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --base sdk/shrink-maps/v<previous>.toml \
  --out  sdk/shrink-maps/v<new>.toml
```

`--base` copies the previous map verbatim — existing entries never get
renamed. See [docs/shrinker.md](/reference/shrinker/) for the full design.

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
