# Getting Started

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

The `build-apk.sh` script compiles Java sources using `javac`. JDK 11 or later is required to run the Java formatter (the formatter tool itself needs JDK 11+, even though app code targets Java 8).

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

# Build firmware with the default example (blinky) for RP2040
./scripts/build.sh

# Flash to Pico and view RTT log output
./scripts/flash.sh
```

### Choosing a chip

Both scripts accept a `--chip` flag. The default is `rp2040`.

| Flag | Target | Board |
|------|--------|-------|
| `--chip rp2040` | `thumbv6m-none-eabi` | Raspberry Pi Pico |
| `--chip rp2350` | `thumbv8m.main-none-eabihf` | Raspberry Pi Pico 2 |

```bash
# Build / flash for Pico 2 (RP2350)
./scripts/build.sh --chip rp2350
./scripts/flash.sh --chip rp2350
```

### Choosing an example

Pass `--app <name>` to select which example to build or flash:

```bash
./scripts/build.sh --app blinky                         # RP2040, default app
./scripts/build.sh --app uart --chip rp2350             # RP2350
./scripts/build.sh --app helloworld --release

./scripts/flash.sh --app blinky
./scripts/flash.sh --app uart --chip rp2350
./scripts/flash.sh --app helloworld --release
```

The `--app` flag selects which example to build. `build.sh` compiles the Java sources into a `.papk` file and embeds it into the firmware — no Cargo feature flags are involved.

## Host Simulator

Run any app on the host machine without hardware using the simulator:

```bash
./scripts/sim.sh --app helloworld
./scripts/sim.sh --app blinky          # loops forever — Ctrl-C to stop
./scripts/sim.sh --app uart --release
```

The simulator builds with `--features sim` and runs natively on the host. Hardware calls (GPIO, UART, I2C, SPI, ADC, PWM) are stubbed with logged output. This is useful for testing app logic without a Pico.

## Hot-Swap with pdb {#hot-swap-with-pdb}

The **Picodroid Debug Bridge** (`pdb`) lets you push a new app to a running device over UART without reflashing the firmware. The firmware listens on UART1 (GP4 TX, GP5 RX) at 115200 baud.

### Quick access via script

```bash
./scripts/pdb.sh devices
./scripts/pdb.sh -s /dev/cu.usbmodem102 ping
./scripts/pdb.sh -s /dev/cu.usbmodem102 install build/apks/blinky.papk
./scripts/pdb.sh -s /dev/cu.usbmodem102 sysmon
```

### Install the host tool globally (optional)

```bash
cargo install --path tools/pdb
```

### Push an app

```bash
# Build the app first
./scripts/build-apk.sh --app blinky

# Find the serial port
pdb devices

# Push the PAPK
pdb -s /dev/cu.usbmodem102 install build/apks/blinky.papk
```

The device stops the running JVM (including any sleeping child threads), writes the new PAPK to flash, and restarts execution — typically in under a second.

### Verify connectivity

```bash
pdb -s /dev/cu.usbmodem102 ping
```

### System monitor

Query heap usage, task states, stack high-water marks, and per-task CPU usage:

```bash
pdb -s /dev/cu.usbmodem102 sysmon
```

CPU % is computed from the delta between consecutive queries. The first query reports CPU % as N/A; run it again after a few seconds to see actual per-task CPU usage.

### Inspect a PAPK file

```bash
cargo run -p papk-info -- build/apks/blinky.papk
```

Prints the manifest, class list, and bytecode size of each class.

### Generating a UF2 file

Pass `--uf2` to `build.sh` to convert the ELF to a UF2 file after building. This is useful for flashing without a debug probe — just drag-and-drop the `.uf2` onto the Pico's USB Mass Storage drive.

```bash
./scripts/build.sh --app blinky --uf2
./scripts/build.sh --app blinky --chip rp2350 --release --uf2
```

The UF2 is written alongside the ELF (e.g. `target/thumbv6m-none-eabi/debug/picodroid.uf2`). Requires `elf2uf2-rs` (`cargo install elf2uf2-rs`).
