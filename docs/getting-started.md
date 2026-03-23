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

The pre-commit hook runs Java formatting check, `cargo fmt -- --check`, Clippy (for both RP2040 and RP2350), `cargo clean && cargo build`, and tests before each commit.

Install the hook after cloning by symlinking so it stays in sync with `scripts/pre-commit`:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

To skip the hook in exceptional cases: `git commit --no-verify`.

### Java Development Kit (JDK 11+)

The build compiles Java sources automatically using `javac`. JDK 11 or later is required to run the Java formatter (the formatter tool itself needs JDK 11+, even though app code targets Java 8).

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

The `--app` flag maps directly to a Cargo feature and controls which Java class is invoked at startup. When omitted, `blinky` is used.

### Generating a UF2 file

Pass `--uf2` to `build.sh` to convert the ELF to a UF2 file after building. This is useful for flashing without a debug probe — just drag-and-drop the `.uf2` onto the Pico's USB Mass Storage drive.

```bash
./scripts/build.sh --app blinky --uf2
./scripts/build.sh --app blinky --chip rp2350 --release --uf2
```

The UF2 is written alongside the ELF (e.g. `target/thumbv6m-none-eabi/debug/picodroid.uf2`). Requires `elf2uf2-rs` (`cargo install elf2uf2-rs`).
