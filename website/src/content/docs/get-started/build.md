---
title: "Build & flash (RP)"
description: "Install the Rust + ARM + Java toolchains, build firmware, and flash a Raspberry Pi Pico."
---

This page walks the Raspberry Pi Pico (RP2040 / RP2350) path.

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
cargo install elf2uf2-rs                # optional: needed for --uf2 on RP2040 boards
```

`--uf2` on an RP2350 board converts with `picotool` instead (`brew install picotool` on macOS).

### Local CI (pre-commit hook)

The pre-commit hook runs `scripts/pre-commit` in its **fast** tier: the source guards that exist only locally (shadow-twin, cfg hygiene, `apply_jvm_env`) plus whichever of the formatters and markdown lint the changed files call for — seconds, with no cargo build. `./scripts/pre-commit --full` is the release-cut gate (the staged and opt-in firmware legs, the shrunk-image check, the binary-size ratchet); everything else runs in GitHub CI on every push and in the nightly sim and HIL runs. `--list` prints the stages a run would execute; `--serial` streams one lane at a time. See [Contributing](/project/contributing/#pre-commit-hook).

Install the hook after cloning by symlinking so it stays in sync with `scripts/pre-commit`:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

To skip the hook in exceptional cases: `git commit --no-verify`.

### Java Development Kit (JDK 21)

App sources are compiled by the Gradle multi-project under the repo root. The `./gradlew` wrapper ships in-tree, so no separate Gradle install is required — only a JDK. App code targets Java 1.8 (configured in `build.gradle.kts`). Install JDK 21: the Java formatter (google-java-format 1.36.1) is a Java 21 jar. CI compiles the apps with Ubuntu's JDK 17 and adds a JDK 21 for the formatter; when `java` on your `PATH` is older, point `JAVA_HOME` at a JDK 21 and `format_java.sh` uses that one.

Compilation is followed by `verifyApiContract`, which fails the build for any
`java.*` class or member pico-jvm does not implement (see the
[compatibility matrix](/reference/compatibility-matrix/) and
[troubleshooting](/guides/troubleshooting/)). `scripts/build-apk.sh --board <name>`
— passed automatically by `build.sh`, `flash.sh` and `sim.sh` — also rejects
classes that board excludes from its framework.

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
# Clone with submodules (third_party/FreeRTOS-Kernel, third_party/lvgl,
# third_party/freertos-plus-tcp, third_party/cyw43-driver)
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs

# Build firmware with the default example (helloworld) for testbench_rp2350
./scripts/build.sh

# Build, flash over the debug probe and stream the RTT log
# (flash.sh's own default example is blinky)
./scripts/flash.sh --app helloworld
```

`flash.sh` does not return: after flashing it keeps printing the device's log until you stop it with Ctrl-C.

Every firmware build ends with a memory report, and fails if the image leaves the core-0 main stack less than 8,192 bytes of RAM:

```text
=== Memory Usage ===
  Flash: <used> / <size> bytes (<n>% of program region; chip total <size>)
  App region: <used> / <size> bytes (embedded PAPK + boot-meta sector; not in Flash:)
  RAM:   <used> / <size> bytes (<n>%)
  Main stack headroom: <n> bytes (floor 8192)
```

`Flash:` is measured against the board's *program region*, not the whole chip: flash is split into the program image, the LittleFS volume and the app region that holds installed apps (`fs_kb` and `app_region_kb` in `board.toml`).

:::caution[Existing checkouts: cyw43-driver moved to a fork]
`third_party/cyw43-driver` now points at the patched picodroid fork. A checkout
cloned before the switch must run

```bash
git submodule sync && git submodule update --init third_party/cyw43-driver
```

or the build fails early with `third_party/cyw43-driver is the unpatched upstream`.
:::

### Choosing a board

Both scripts accept a `--board` flag. The default is `testbench_rp2350`.

| Flag | MCU | Target |
|------|-----|--------|
| `--board testbench_rp2040` | RP2040 | Raspberry Pi Pico with a Waveshare 2.8" display (320x240 ST7789, XPT2046 resistive touch). Single-app; no networking |
| `--board testbench_rp2350` | RP2350 | Raspberry Pi Pico 2 with the same Waveshare display |
| `--board testbench_rp2350w` | RP2350 | Raspberry Pi Pico 2 W with the same display — adds WiFi (cyw43 + FreeRTOS+TCP) and HTTPS |
| `--board pico_enviro_mon` | RP2350 | Pico Enviro Mon: Pimoroni Pico Enviro+ Pack (240x240 ST7789, four buttons, no touch, BME688 and LTR559 sensors) |
| `--board pico_enviro_mon_w` | RP2350 | Pico Enviro Mon on a Pico 2 W — same wiring plus WiFi and HTTPS |
| `--board pico_display2_w` | RP2350 | Pimoroni Pico Display Pack 2.0 (320x240 ST7789, four buttons, no touch) on a Pico 2 W, with WiFi and HTTPS. Built for the `claudeusage` example |
| `--board pico_touch_kit` | RP2350B | 52Pi EP-0172 carrier on a Pimoroni Pico Plus 2 W (3.5" 320x480 ST7796, GT911 capacitive touch, BACK and HOME buttons, buzzer, WiFi and HTTPS, 16 MB flash) |

Every RP2350 board is a **multi-app** board: its firmware carries the launcher and the settings app, and its app region holds several installed apps (up to eight; sixteen on `pico_touch_kit`). `testbench_rp2040` holds one app. A WiFi board joins the network you pick in Settings → Wi-Fi, or the one named by build-time credentials — see [WiFi & networking setup](/get-started/networking/).

| Board | Program region | Filesystem | App region |
|-------|----------------|------------|------------|
| `testbench_rp2040` | 1152 KB | 128 KB | 768 KB |
| `testbench_rp2350`, `pico_enviro_mon` | 2048 KB | 512 KB | 1536 KB |
| `testbench_rp2350w`, `pico_enviro_mon_w`, `pico_display2_w` | 2304 KB | 512 KB | 1280 KB |
| `pico_touch_kit` | 2048 KB | 4096 KB | 10240 KB |

```bash
# Build / flash for Pico (RP2040)
./scripts/build.sh --board testbench_rp2040
./scripts/flash.sh --board testbench_rp2040
```

On a bench with several boards (described by `~/.config/picodroid/fleet.conf`), `flash.sh --board <name>` also says which physical board to flash: the script takes a lease on that board before it builds, and exits with code 75, naming the holder, when another session has it. With one board and no `fleet.conf` there is nothing to set up. See [Sharing the bench](/project/contributing/#sharing-the-bench).

For day-to-day work, the per-board cargo aliases (`cargo b-testbench-rp2040`, `cargo r-testbench-rp2350w`, etc.) skip the script and call `cargo` directly — see [Cargo aliases](/reference/cargo-aliases/).

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

### Choosing what boots

On a multi-app board the app `--app` names is installed in the app region and is what the device boots. When it finishes — its last Activity is closed, BACK on its root screen included — the launcher built into the firmware takes over and lists what is installed. `flash.sh --boot` picks something else to boot:

```bash
./scripts/flash.sh --app blinky --boot launcher    # boot the launcher, blinky installed beside it
```

`--boot` takes `app` (the default), `launcher`, or the package name of an installed app; a name that is not installed falls back to the usual rule. More apps are added over USB with [`pdb install`](/get-started/hot-swap/). See the [launcher guide](/guides/launcher/).

### Shrinking

`build.sh`, `flash.sh` and `sim.sh` accept `--shrink`, which applies the active release shrink map (off by default): framework class names — `picodroid.*` and `java.*` alike — and method/field names become one- or two-character synthetic names in the firmware and in the PAPK, ProGuard-style. An app name that happens to spell one of those short names is renamed out of the way. `--shrink-app` on top renames the app's own classes and private members as well; it requires `--shrink`. In both modes a build that renamed app names writes the merged map next to the PAPK as `build/apks/<app>.shrink-map.toml`:

```bash
./scripts/build.sh --app helloworld --release --shrink
./scripts/flash.sh --app helloworld --release --shrink
./scripts/flash.sh --app helloworld --release --shrink --shrink-app
```

Firmware and PAPK must be built with the same `--shrink` setting, or the install is rejected with a version mismatch — see [Shrinker](/reference/shrinker/). A shrunk firmware prints the mapped names in `Class.getName()`, stack traces and logs; `./scripts/retrace.sh < log` (with the per-app map for a `--shrink-app` build) turns them back into the originals.

Independently of shrinking, every device build strips the `.class` debug attributes pico-jvm never reads on a device. Debug-profile builds keep source line numbers (the `line-numbers` cargo feature, `build-apk.sh --keep-lines`), so an uncaught exception prints `at pkg.Class.method(File.java:42)`; `--release` builds, and every `testbench_rp2040` build (its program region has no room for the tables), print the bytecode offset, `(pc=N)`, which `./scripts/retrace.sh --app <name> < log` resolves on the host. `PICODROID_LINE_NUMBERS=0|1` overrides either way. See [Debugging](/guides/debugging/#stack-traces-and-shrunk-logs).

## Generating a UF2 file

Pass `--uf2` to `build.sh` to convert the ELF to a UF2 file after building. This is useful for flashing without a debug probe — just drag-and-drop the `.uf2` onto the Pico's USB Mass Storage drive.

```bash
./scripts/build.sh --app blinky --uf2
./scripts/build.sh --app blinky --board testbench_rp2350 --release --uf2
```

The UF2 is written alongside the ELF as `picodroid.uf2` (e.g. `target/thumbv8m.main-none-eabihf/debug/picodroid.uf2` for an RP2350 board, `target/thumbv6m-none-eabi/debug/picodroid.uf2` for the RP2040). An RP2350 board needs `picotool` for the conversion, the RP2040 `elf2uf2-rs` (`cargo install elf2uf2-rs`).

## Next steps

- [Host simulator](/get-started/simulator/) — run apps on your dev machine without hardware.
- [Hot-swap with pdb](/get-started/hot-swap/) — push a new app over USB CDC without reflashing.
- [Your first app](/get-started/first-app/) — scaffold a Java app and wire its lifecycle.
