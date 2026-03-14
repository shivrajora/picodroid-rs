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

## Examples

Five examples are included under `java/examples/`:

| Example | Class | Description |
|---------|-------|-------------|
| `blinky` | `blinky.LedBlink` | Blinks the onboard LED on GP25 every 500 ms |
| `uart` | `uart.UartEcho` | Configures UART0 at 115200 baud and echoes received bytes |
| `helloworld` | `helloworld.HelloWorld` | Prints "Hello, World!" via `Log.i()` |
| `arraydemo` | `arraydemo.ArrayDemo` | Demonstrates byte array allocation, `.length`, and iteration via UART |
| `inherit` | `inherit.InheritDemo` | Demonstrates class inheritance, field inheritance, method overriding, and `super()` |

## Getting Started

```bash
# Clone with submodules (FreeRTOS-Kernel)
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs

# Build firmware with the default example (blinky)
./scripts/build.sh

# Flash to Pico and view RTT log output
./scripts/flash.sh
```

### Choosing an example

Pass `--app <name>` to select which example to build or flash:

```bash
./scripts/build.sh --app blinky       # default
./scripts/build.sh --app uart
./scripts/build.sh --app helloworld
./scripts/build.sh --app arraydemo
./scripts/build.sh --app inherit

./scripts/flash.sh --app uart
./scripts/flash.sh --app helloworld --release
./scripts/flash.sh --app arraydemo
./scripts/flash.sh --app inherit
```

The `--app` flag maps directly to a Cargo feature and controls which Java class is invoked at startup.

## Writing a Java App

1. Create a directory and Java file under `java/examples/`:

```java
// java/examples/myapp/java/myapp/MyApp.java
package myapp;

import picodroid.util.Log;

public class MyApp {
    public static void main(String[] args) {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

2. Add a feature for it in `Cargo.toml`:

```toml
[features]
default = ["blinky"]
helloworld = []
blinky = []
uart = []
myapp = []       # add this
```

3. Add a `cfg(feature)` block in `src/framework/mod.rs` inside `run_jvm()`:

```rust
#[cfg(feature = "myapp")]
{
    jvm.load_class(MYAPP_MYAPP_CLASS).unwrap();
    jvm.load_class(PICODROID_UTIL_LOG_CLASS).unwrap();
    jvm.invoke_static("myapp/MyApp", "main", &mut handler).unwrap();
}
```

4. Build and flash:

```bash
./scripts/build.sh --app myapp
./scripts/flash.sh --app myapp
```

The build system automatically detects new `.java` files, compiles them with `javac`, and embeds the resulting `.class` bytecode into firmware Flash. The constant name for a class is derived from its path: `java/examples/myapp/java/myapp/MyApp.class` → `MYAPP_MYAPP_CLASS`.

### Supported language features

| Feature | Example |
|---------|---------|
| Arrays | `byte[] buf = new byte[16];` — allocation, `.length`, index read/write |
| Inheritance | `class Dog extends Animal` — field inheritance, `@Override`, `super()` constructor chaining, virtual dispatch |

## Java System API

Java system APIs live under `java/framework/java/picodroid/` and mirror the Android API surface. Native implementations are in `src/system/picodroid/`.

### `picodroid.util.Log`

```java
import picodroid.util.Log;

Log.i("TAG", "message");   // info log → defmt::info! over RTT
```

### `picodroid.os.SystemClock`

```java
import picodroid.os.SystemClock;

SystemClock.sleep(500);   // sleep for 500 ms
```

### `picodroid.pio.PeripheralManager`

Singleton for opening hardware peripherals.

```java
import picodroid.pio.PeripheralManager;

PeripheralManager pm = PeripheralManager.getInstance();
Gpio gpio = pm.openGpio("BCM25");
UartDevice uart = pm.openUartDevice("UART0");
```

### `picodroid.pio.Gpio`

```java
import picodroid.pio.Gpio;

gpio.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
gpio.setValue(true);    // drive high
gpio.setValue(false);   // drive low
```

### `picodroid.pio.UartDevice`

```java
import picodroid.pio.UartDevice;

uart.setBaudrate(115200);
byte[] buf = new byte[1];
uart.read(buf, 1);      // blocking read
uart.write(buf, 1);     // write byte
```

## Project Structure

```
picodroid-rs/
├── java/
│   ├── framework/      # Android-compatible Java API stubs (picodroid.*)
│   └── examples/       # Example apps (blinky, uart, helloworld, arraydemo, inherit)
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
