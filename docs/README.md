# Picodroid Documentation

A FreeRTOS-based Android-like runtime for the Raspberry Pi Pico, with apps written in Java and run on a Rust-built JVM. See the top-level [README](../README.md) for the project overview.

## Start here

If you're new, read in this order:

1. **[Getting Started](getting-started.md)** — install the toolchain, build, flash, pick a board, run the simulator, hot-swap apps with pdb.
2. **[Writing Apps](writing-apps.md)** — create a new Java app, the `Application` / `Activity` lifecycle, and the supported language features.
3. **[Examples](examples.md)** — 32 example apps grouped by category. Pick one close to what you want to build and copy it.
4. **API Reference** — split by area; jump straight to the area you need:
   - [Core language](api/core.md) — `String`, `StringBuilder`, `Math`, `ArrayList`, `HashMap`, `HashSet`, `Iterator`, enums
   - [System services](api/system.md) — `Log`, `SystemClock`, `Runtime` (GC), `Thread`
   - [Peripherals](api/peripherals.md) — GPIO, UART, I2C, SPI, PWM, ADC
   - [Storage](api/storage.md) — files (`picodroid.io`) and preferences (`picodroid.content`)
   - [Networking](api/networking.md) — TCP / UDP sockets (Pico 2 W on hardware; sim always works)
   - [Graphics & UI](api/ui.md) — `Application` / `Activity` lifecycle, `Display`, all 14 widgets

## I want to…

| Goal | Read |
|------|------|
| Blink an LED | [Examples → blinky](examples.md#getting-started) → [api/peripherals.md](api/peripherals.md) |
| Talk to an I2C / SPI sensor | [api/peripherals.md](api/peripherals.md) (`I2cDevice`, `SpiDevice`) |
| Persist settings across reboots | [api/storage.md](api/storage.md) (`Preferences`) |
| Read or write files | [api/storage.md](api/storage.md) (`File`, `FileInputStream`, `FileOutputStream`) |
| Open a TCP/UDP socket over WiFi | [api/networking.md](api/networking.md) — needs `--board testbench_rp2350w` on hardware |
| Build a touchscreen UI | [api/ui.md](api/ui.md) (`Activity`, widgets) — also try [examples → displaydemo](examples.md#graphics-and-display) |
| Spawn a background thread | [api/system.md](api/system.md#picodroidconcurrentthread) (`picodroid.concurrent.Thread`) |
| Push a new app without reflashing | [Getting Started → Hot-Swap with pdb](getting-started.md#hot-swap-with-pdb) |
| Run an app without hardware | [Getting Started → Host Simulator](getting-started.md#host-simulator) |
| Diagnose a hang or measure CPU/heap | [Debugging](debugging.md) (`pdb sysmon`, RTT, GDB) |
| Hit a build / flash / port error | [Troubleshooting](troubleshooting.md) |
| Add support for a new MCU | [Porting Guide](porting-guide.md) |

## Reference

- [Java API index](java-api.md) — one-page index of the API split above
- [Class-name Shrinker](shrinker.md) — release-tied, append-only shrink maps; how firmware and PAPKs pick up the active map
- [Porting Guide](porting-guide.md) — HAL function signatures, FreeRTOS config, build system
- [RP2350 FreeRTOS SMP Bugs](rp2350-freertos-smp-bugs.md) — known issues with the SMP port on RP2350
- [Debugging](debugging.md) — RTT logging, the host simulator, `pdb sysmon`, GDB
- [Troubleshooting](troubleshooting.md) — common pitfalls and solutions
- [Contributing](../CONTRIBUTING.md) — how to contribute, run tests, add examples or native methods
