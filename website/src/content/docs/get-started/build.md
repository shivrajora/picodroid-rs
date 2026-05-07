---
title: "Build & flash (RP)"
description: "Install the Rust + ARM + Java toolchains, build firmware, and flash a Raspberry Pi Pico."
---

This page walks the Raspberry Pi Pico (RP2040 / RP2350) path. For ESP32-S3 / Lilygo T-Deck Plus, see [ESP32-S3 quickstart](/get-started/esp32s3/).

## Prerequisites

### Rust toolchain

```bash
# RP2040 (Cortex-M0+)
rustup target add thumbv6m-none-eabi

# RP2350 (Cortex-M33) — only needed if targeting Pico 2
rustup target add thumbv8m.main-none-eabihf
```

### C cross-compiler (for FreeRTOS)

```bash
# macOS
brew install arm-none-eabi-gcc

# Ubuntu/Debian
sudo apt install gcc-arm-none-eabi
```

### Rust tools

```bash
cargo install flip-link
cargo install probe-rs-tools --locked   # installs probe-rs
cargo install elf2uf2-rs                # optional: needed for --uf2 flag
```

### Local CI (pre-commit hook)

The pre-commit hook runs Java formatting check, `cargo fmt`, APK build, Clippy (RP2040, RP2350, and sim), firmware build, and tests before each commit.

Install the hook after cloning by symlinking so it stays in sync with `scripts/pre-commit`:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

To skip the hook in exceptional cases: `git commit --no-verify`.

### Java Development Kit (JDK 11+)

App sources are compiled by the Gradle multi-project under the repo root. The `./gradlew` wrapper ships in-tree, so no separate Gradle install is required — only a JDK. App code targets Java 1.8 (configured in `build.gradle.kts`), but JDK 11 or later is required because the Java formatter tool needs it.

```bash
# macOS
brew install openjdk
brew link openjdk --force

# Ubuntu/Debian
sudo apt install default-jdk
```

Verify: `javac --version`

### Java formatting (google-java-format)

Java source files must follow [Google Java Style](https://google.github.io/styleguide/javaguide.html). No separate installation needed — the formatter JAR is downloaded automatically on first use.

```bash
# Reformat all Java files in-place
./scripts/format_java.sh format

# Check formatting without modifying files (runs in pre-commit hook and CI)
./scripts/format_java.sh check
```

## Building and Flashing

```bash
# Clone with submodules (FreeRTOS-Kernel)
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs

# Build firmware with the default example (blinky) for testbench_rp2350
./scripts/build.sh

# Flash to Pico and view RTT log output
./scripts/flash.sh
```

### Choosing a board

Both scripts accept a `--board` flag. The default is `testbench_rp2350`.

| Flag | MCU | Target |
|------|-----|--------|
| `--board testbench_rp2040` | RP2040 | Raspberry Pi Pico |
| `--board testbench_rp2350` | RP2350 | Raspberry Pi Pico 2 |
| `--board testbench_rp2350w` | RP2350 | Raspberry Pi Pico 2 W (adds WiFi via cyw43 + FreeRTOS+TCP) |
| `--board pico_enviro_mon` | RP2350 | Pico Enviro Mon (1.14" 240x135 ST7789, no touch) |
| `--board tdeck_plus` | ESP32-S3 | Lilygo T-Deck Plus — see [ESP32-S3 quickstart](/get-started/esp32s3/) |

```bash
# Build / flash for Pico (RP2040)
./scripts/build.sh --board testbench_rp2040
./scripts/flash.sh --board testbench_rp2040
```

For day-to-day work, the per-board cargo aliases (`cargo b-rp2040`, `cargo r-rp2350w`, etc.) skip the script and call `cargo` directly — see [Cargo aliases](/reference/cargo-aliases/).

### Choosing an example

Pass `--app <name>` to select which example to build or flash:

```bash
./scripts/build.sh --app blinky                                   # default board
./scripts/build.sh --app uart --board testbench_rp2040            # RP2040
./scripts/build.sh --app helloworld --release

./scripts/flash.sh --app blinky
./scripts/flash.sh --app uart --board testbench_rp2040
./scripts/flash.sh --app helloworld --release
```

The `--app` flag selects which example to build. `build.sh` compiles the Java sources into a `.papk` file and embeds it into the firmware — no Cargo feature flags are involved.

## Generating a UF2 file

Pass `--uf2` to `build.sh` to convert the ELF to a UF2 file after building. This is useful for flashing without a debug probe — just drag-and-drop the `.uf2` onto the Pico's USB Mass Storage drive.

```bash
./scripts/build.sh --app blinky --uf2
./scripts/build.sh --app blinky --board testbench_rp2350 --release --uf2
```

The UF2 is written alongside the ELF (e.g. `target/thumbv6m-none-eabi/debug/picodroid.uf2`). Requires `elf2uf2-rs` (`cargo install elf2uf2-rs`).

## Next steps

- [Host simulator](/get-started/simulator/) — run apps on your dev machine without hardware.
- [Hot-swap with pdb](/get-started/hot-swap/) — push a new app over USB CDC without reflashing.
- [Your first app](/get-started/first-app/) — scaffold a Java app and wire its lifecycle.
