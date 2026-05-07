<p align="center">
  <img src="assets/picodroid_rs_inverted.svg" alt="Picodroid" width="200"/>
</p>

<p align="center">

[![CI](https://github.com/shivrajora/picodroid-rs/actions/workflows/ci_checks.yml/badge.svg)](https://github.com/shivrajora/picodroid-rs/actions/workflows/ci_checks.yml)
![License](https://img.shields.io/badge/license-GPL--3.0--only-blue)
![Rust](https://img.shields.io/badge/rust-nightly%20%7C%20stable-orange)

</p>

# Picodroid

A stripped-down, FreeRTOS-based version of Android for the Raspberry Pi Pico.

Apps are written in Java, compiled to bytecode, and interpreted by a lightweight JVM built in Rust — running directly on bare-metal embedded hardware.

## What is Picodroid?

| Layer | Technology |
|-------|-----------|
| Hardware | Raspberry Pi Pico (RP2040, dual Cortex-M0+ @ 125 MHz) or Pico 2 (RP2350, dual Cortex-M33 @ 150 MHz) |
| RTOS | FreeRTOS SMP — both cores active (via [freertos-rust](https://github.com/shivrajora/FreeRTOS-rust)) |
| Runtime | Custom JVM interpreter in Rust (`jvm/` library crate) |
| Java API | Android-compatible: `picodroid.util.Log`, `picodroid.widget.*` (LVGL-backed UI — incl. `Toast` / `AlertDialog` / `Keyboard`), `picodroid.view.{KeyEvent, GestureDetector, ViewPropertyAnimator}`, `picodroid.graphics.{Theme, drawable.GradientDrawable}`, `picodroid.app.Activity` (full lifecycle + back stack), `picodroid.io` (LittleFS files), `picodroid.content.Preferences`, `picodroid.net` (TCP/UDP + `HttpUrlConnection` over WiFi on Pico 2 W), `picodroid.hardware.SensorManager` (BME688), `picodroid.concurrent.Thread` / `Executors`, etc. |
| Logging | [defmt](https://defmt.ferrous-systems.com/) over RTT |

### Architecture

```mermaid
graph TD
    subgraph Hardware
        HW["RP2040 (Cortex-M0+ @ 125 MHz) / RP2350 (Cortex-M33 @ 150 MHz)"]
    end

    subgraph FreeRTOS["FreeRTOS SMP — both cores"]
        PDB["pdb task<br/><i>priority 2, core 1</i>"]
        JVM_TASK["jvm task<br/><i>priority 1, core 0</i>"]
    end

    subgraph JVM["JVM Interpreter (jvm/ crate)"]
        BC["Java bytecode<br/>.papk app"]
        NATIVE["Native dispatch<br/>GPIO / UART / I2C / SPI / Log / Display / Net / FS (LittleFS)"]
        THREADS["Thread.start()<br/>child tasks (core 0)"]
        GC["Mark-sweep GC<br/>every 256 allocs"]
    end

    subgraph Host["Host (development machine)"]
        PDB_CLI["pdb CLI tool"]
    end

    HW --> FreeRTOS
    JVM_TASK --> JVM
    BC --> NATIVE
    BC --> THREADS
    BC --> GC

    PDB_CLI -- "USB CDC hot-swap" --> PDB
    PDB -- "write .papk → Flash<br/>restart JVM" --> JVM_TASK
```

Apps can be hot-swapped at runtime via `pdb install` without reflashing the firmware.

## Hardware

- Raspberry Pi Pico (RP2040) or Raspberry Pi Pico 2 (RP2350)
- An SWD debug probe: [Raspberry Pi Debug Probe](https://www.raspberrypi.com/products/debug-probe/), Picoprobe, J-Link, or any CMSIS-DAP adapter

## Quick Start

```bash
git clone --recurse-submodules https://github.com/shivrajora/picodroid-rs
cd picodroid-rs
./scripts/build.sh --app helloworld
./scripts/flash.sh --app helloworld
```

After flashing, push a new app over USB CDC without reflashing:

```bash
cargo run -p pdb -- -s /dev/cu.usbmodem102 install build/apks/blinky.papk
```

Check device health (heap, tasks, CPU usage) at any time:

```bash
cargo run -p pdb -- -s /dev/cu.usbmodem102 sysmon
```

Display apps (e.g. `displaydemo`) open a graphical window with mouse-as-touch input when run in the simulator.

See [Build & flash](website/src/content/docs/get-started/build.md) for prerequisites, chip selection, app selection, and UF2 flashing.

## Documentation

The full docs are an Astro Starlight site under [`website/`](website/) — once the GitHub Pages workflow ships, they're hosted at `https://shivrajora.github.io/picodroid-rs/`. The same content also renders on GitHub directly:

- [Get started → Build & flash](website/src/content/docs/get-started/build.md) — prerequisites, build, flash, board/app selection
- [Get started → Host simulator](website/src/content/docs/get-started/simulator.md) — run apps without hardware
- [Get started → Hot-swap with pdb](website/src/content/docs/get-started/hot-swap.md) — push PAPKs over USB CDC
- [Get started → Your first app](website/src/content/docs/get-started/first-app.md) — Application/Activity lifecycle and supported language features
- [ESP32-S3 quickstart](website/src/content/docs/get-started/esp32s3.md) — Lilygo T-Deck Plus M1 (compile-only)
- [Java API](website/src/content/docs/api.md) — split by area: [core](website/src/content/docs/api/core.md), [system](website/src/content/docs/api/system.md), [services](website/src/content/docs/api/services.md), [peripherals](website/src/content/docs/api/peripherals.md), [storage](website/src/content/docs/api/storage.md), [networking](website/src/content/docs/api/networking.md), [sensors](website/src/content/docs/api/sensors.md), [UI](website/src/content/docs/api/ui.md)
- [Bundled image assets](website/src/content/docs/guides/assets.md) — PAPK 1.1 ASST, `ImageView.setImageSource`
- [Class-name shrinker](website/src/content/docs/reference/shrinker.md) — opt-in (`--shrink`) release-tied, append-only maps
- [Release notes](website/src/content/docs/project/release-notes.md) — v0.4.0 → v0.9.0
- [Contributing](CONTRIBUTING.md) — how to contribute, run tests, and add new features

## Project Structure

```text
picodroid-rs/
├── jvm/                # JVM interpreter — reusable library crate (pico-jvm)
│   └── src/            # no_std + alloc only; no hardware dependencies
│
├── sdk/                # Android-compatible Java API stubs (picodroid.*)
│   ├── java/           # Framework Java sources (compiled into firmware Flash)
│   ├── keep.toml       # Class-name shrinker keep list
│   └── shrink-maps/    # Immutable per-release shrink maps (v<semver>.toml)
│
├── examples/           # Example apps, each with Java sources and a PicodroidManifest.xml
│
├── src/
│   ├── app.rs          # JVM bootstrap (run_jvm, shared heap, class loader)
│   ├── lifecycle.rs    # Application/Activity lifecycle management
│   ├── lvgl_ffi.rs     # FFI bindings to the LVGL graphics library
│   ├── drivers/        # Display and touch hardware drivers (ST7789, XPT2046)
│   ├── boards/         # Board-specific pin and peripheral configurations
│   ├── hal/            # Hardware Abstraction Layer (rp/ for Pico, sim/ for host simulator)
│   │   └── rp/port/    # pico-sdk C shims (headers + FreeRTOS interop shims)
│   ├── packagemanager/ # Flash storage and PAPK install logic
│   ├── pdb/            # Picodroid Debug Bridge — UART listener + hot-swap logic
│   └── system/         # Native implementations of Java API methods
│
├── tools/
│   ├── papk-pack/      # Host tool: packages compiled .class files into a .papk file
│   ├── papk-info/      # Host tool: inspect .papk file contents (manifest, classes, sizes)
│   ├── class-shrink/   # Host tool: release-tied class-name shrinker (see docs/shrinker.md)
│   └── pdb/            # Host tool: push apps and monitor device health over USB CDC
│
├── scripts/            # Build, flash, sim, pdb, test, and pre-commit scripts
│
├── vendor/             # Downloaded tooling and libraries (google-java-format JAR, LVGL; gitignored)
│
├── memory.x            # RP2040 linker memory layout
├── memory_rp2350.x     # RP2350 linker memory layout
├── third_party/        # Git submodules (FreeRTOS-Kernel)
└── build.rs            # Compiles FreeRTOS C, embeds pre-built .papk into firmware Flash
```

## Attribution

Project scaffolding based on [rp2040-project-template](https://github.com/rp-rs/rp2040-project-template).

## License

picodroid is dual-licensed:

- **Open source:** [GPL-3.0-only](LICENSE) (no Classpath Exception). Forks,
  modifications, and any Java app linking the picodroid SDK must release
  source under GPL-3.0.
- **Commercial:** A separate proprietary license is available for customers
  who need to ship closed-source apps or derivatives. See [LICENSING.md](LICENSING.md)
  for details and contact info.

Contributors: see [CONTRIBUTING.md](CONTRIBUTING.md) and [CLA.md](CLA.md) —
opening a PR constitutes agreement to the inbound license grant that keeps
the dual-license model possible.
