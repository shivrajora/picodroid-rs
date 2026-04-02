# Contributing to Picodroid

## Getting Set Up

See [docs/getting-started.md](docs/getting-started.md) for full prerequisites (Rust toolchain, ARM cross-compiler, JDK 11+, probe-rs).

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

2. Write your Java source with a `public static void main(String[] args)` entry point:

```java
package myapp;

import picodroid.util.Log;

public class MyApp {
    public static void main(String[] args) {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

3. Create `PicodroidManifest.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="myapp" version="1.0">
    <application main-class="myapp/MyApp" />
</manifest>
```

4. Build and test:

```bash
./scripts/build.sh --app myapp
./scripts/sim.sh --app myapp        # test on host first
./scripts/flash.sh --app myapp      # flash to hardware
```

5. Add your app to `docs/examples.md` in the appropriate category.

See [docs/writing-apps.md](docs/writing-apps.md) for supported language features and the full Java API.

## Adding a New Native Java Method

When adding a new native method that the JVM dispatches to Rust:

1. Add the native implementation in `src/system/` under the appropriate module
2. Register the method in the `NativeMethodHandler` dispatch in `src/system/`
3. If adding a new class to `BuiltinHandler`, also register it in `class_name_to_static_in` in `jvm/src/helpers.rs` — otherwise virtual dispatch will silently break
4. Add the Java API stub in `sdk/java/picodroid/`
5. Update `docs/java-api.md` with the new API surface

## Submitting Changes

1. Make sure `./scripts/pre-commit` passes with `==> All checks passed.`
2. Test your changes with the simulator (`./scripts/sim.sh`) and on hardware if possible
3. Keep commits focused — one logical change per commit
4. Open a pull request with a clear description of what changed and why

## License

By contributing, you agree that your contributions will be licensed under the Apache-2.0 license.
