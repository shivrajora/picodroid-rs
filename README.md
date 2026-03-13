# Picodroid

A stripped-down, FreeRTOS-based version of Android for the Raspberry Pi Pico.

Apps are written in Java, compiled to bytecode, and interpreted by a lightweight JVM built in Rust — running directly on bare-metal embedded hardware.

## What is Picodroid?

| Layer | Technology |
|-------|-----------|
| Hardware | Raspberry Pi Pico (RP2040, Cortex-M0+ @ 125 MHz) |
| RTOS | FreeRTOS (via [freertos-rust](https://github.com/shivrajora/FreeRTOS-rust)) |
| Runtime | Custom JVM interpreter in Rust (`src/framework/`) |
| Java API | Android-compatible (`picodroid.util.Log`, etc.) |
| Logging | [defmt](https://defmt.ferrous-systems.com/) over RTT |

### Execution flow

```
FreeRTOS scheduler
  └── "jvm" task
       └── JVM interpreter (src/framework/)
            └── Java bytecode (.class embedded in Flash)
                 └── native dispatch → defmt RTT log
```

## Hardware

- Raspberry Pi Pico (or any RP2040 board)
- An SWD debug probe: [Raspberry Pi Debug Probe](https://www.raspberrypi.com/products/debug-probe/), Picoprobe, J-Link, or any CMSIS-DAP adapter

## Prerequisites

### Rust toolchain

```bash
rustup target add thumbv6m-none-eabi
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
```

### Local CI (pre-commit hook)

The pre-commit hook runs Java formatting check, `cargo fmt -- --check`, and `cargo clean && cargo build` before each commit.

Install the hook after cloning:

```bash
cp scripts/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
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

## Getting Started

```bash
# Clone with submodules (FreeRTOS-Kernel)
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs

# Build firmware (also compiles Java sources) and print memory usage stats
./scripts/build.sh

# Flash to Pico and view RTT log output
cargo run
```

Expected output:

```
0.001000 INFO  HelloWorld: Hello, World!
```

## Writing a Java App

1. Create a Java file under `java/apps/` with the appropriate package:

```java
// java/apps/MyApp.java
package apps;

import picodroid.util.Log;

public class MyApp {
    public static void main(String[] args) {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

2. Register it in `src/framework/mod.rs` inside `run_jvm()`:

```rust
jvm.load_class(APPS_MYAPP_CLASS).unwrap();
jvm.invoke_static("apps/MyApp", "main").unwrap();
```

3. Rebuild and flash:

```bash
./scripts/build.sh && cargo run
```

The build system automatically detects new `.java` files, compiles them with `javac`, and embeds the resulting `.class` bytecode into firmware Flash.

## Java System API

Java system APIs live under `java/picodroid/` and mirror the Android API surface.

### `picodroid.util.Log`

```java
import picodroid.util.Log;

Log.i("TAG", "message");   // info log → defmt::info! over RTT
```

Native implementations are in `src/system/picodroid/util/log.rs`.

## Project Structure

```
picodroid-rs/
├── java/               # Java source root
│   ├── apps/           # Your Java apps go here
│   └── picodroid/      # Android-compatible Java API stubs
│
├── src/
│   ├── framework/      # JVM interpreter (Rust)
│   └── system/         # Native implementations of Java API methods
│
├── third_party/        # Git submodules (FreeRTOS-Kernel)
└── build.rs            # Compiles FreeRTOS C + Java sources, embeds .class into firmware
```

## Debugging

`cargo run` flashes the firmware and streams RTT log output via [defmt](https://defmt.ferrous-systems.com/) and probe-rs. Log levels are controlled by `DEFMT_LOG` (set to `debug` by default in `.cargo/config.toml`).

For GDB debugging, run probe-rs in GDB server mode and connect with:

```bash
arm-none-eabi-gdb target/thumbv6m-none-eabi/debug/picodroid
```

## Attribution

Project scaffolding based on [rp2040-project-template](https://github.com/rp-rs/rp2040-project-template).

## License

Apache-2.0
