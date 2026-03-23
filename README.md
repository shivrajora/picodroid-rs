# Picodroid

A stripped-down, FreeRTOS-based version of Android for the Raspberry Pi Pico.

Apps are written in Java, compiled to bytecode, and interpreted by a lightweight JVM built in Rust — running directly on bare-metal embedded hardware.

## What is Picodroid?

| Layer | Technology |
|-------|-----------|
| Hardware | Raspberry Pi Pico (RP2040, Cortex-M0+ @ 125 MHz) or Pico 2 (RP2350, Cortex-M33 @ 150 MHz) |
| RTOS | FreeRTOS (via [freertos-rust](https://github.com/shivrajora/FreeRTOS-rust)) |
| Runtime | Custom JVM interpreter in Rust (`jvm/` library crate) |
| Java API | Android-compatible (`picodroid.util.Log`, etc.) |
| Logging | [defmt](https://defmt.ferrous-systems.com/) over RTT |

### Execution flow

```
FreeRTOS scheduler
  └── "jvm" task
       └── JVM interpreter (jvm/ crate)
            └── Java bytecode (.class embedded in Flash)
                 └── native dispatch → defmt RTT log
```

## Hardware

- Raspberry Pi Pico (RP2040) or Raspberry Pi Pico 2 (RP2350)
- An SWD debug probe: [Raspberry Pi Debug Probe](https://www.raspberrypi.com/products/debug-probe/), Picoprobe, J-Link, or any CMSIS-DAP adapter

## Quick Start

```bash
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs
./scripts/build.sh
./scripts/flash.sh
```

See [docs/getting-started.md](docs/getting-started.md) for prerequisites, chip selection, app selection, and UF2 flashing.

## Documentation

- [Getting Started](docs/getting-started.md) — prerequisites, build, flash, chip and app selection
- [Examples](docs/examples.md) — all included example apps
- [Writing Apps](docs/writing-apps.md) — how to create a new Java app, supported language features, and porting to a new platform
- [Java API](docs/java-api.md) — `picodroid.*` system API reference
- [Debugging](docs/debugging.md) — RTT logging and GDB

## Project Structure

```
picodroid-rs/
├── jvm/                # JVM interpreter — reusable library crate (pico-jvm)
│   └── src/            # no_std + alloc only; no hardware dependencies
│
├── java/
│   ├── framework/      # Android-compatible Java API stubs (picodroid.*)
│   └── examples/       # Example apps (blinky, uart, helloworld, arraydemo, inherit, interfacedemo, floatdemo, exceptiondemo, threaddemo, mathsdemo)
│
├── src/
│   ├── app.rs          # JVM bootstrap (run_jvm, shared heap, class loader)
│   └── system/         # Native implementations of Java API methods
│
├── memory.x            # RP2040 linker memory layout
├── memory_rp2350.x     # RP2350 linker memory layout
├── third_party/        # Git submodules (FreeRTOS-Kernel)
└── build.rs            # Compiles FreeRTOS C + Java sources, embeds .class into firmware
```

## Attribution

Project scaffolding based on [rp2040-project-template](https://github.com/rp-rs/rp2040-project-template).

## License

Apache-2.0
