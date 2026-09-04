<p align="center">
  <img src="assets/picodroid.svg" alt="Picodroid" width="200"/>
</p>

<p align="center">
  <a href="https://github.com/shivrajora/picodroid-rs/actions/workflows/ci_checks.yml"><img src="https://github.com/shivrajora/picodroid-rs/actions/workflows/ci_checks.yml/badge.svg" alt="CI"/></a>
  <img src="https://img.shields.io/badge/license-GPL--3.0--only-blue" alt="License: GPL-3.0-only"/>
  <img src="https://img.shields.io/badge/rust-nightly%20%7C%20stable-orange" alt="Rust: nightly | stable"/>
</p>

<p align="center">
  <a href="https://shivrajora.github.io/picodroid-rs/"><b>Documentation</b></a>
</p>

# Picodroid

A stripped-down, FreeRTOS-based version of Android for the Raspberry Pi Pico.

Apps are written in Java, compiled to bytecode, and interpreted by a lightweight JVM built in Rust — running directly on bare-metal embedded hardware.

## What is Picodroid?

| Layer | Technology |
|-------|-----------|
| Hardware | Raspberry Pi Pico (RP2040, dual Cortex-M0+ @ 125 MHz), Pico 2 (RP2350, dual Cortex-M33 @ 150 MHz), or Pico 2 W (RP2350 + CYW43439 WiFi) |
| RTOS | FreeRTOS SMP — both cores active (via [freertos-rust](https://github.com/shivrajora/FreeRTOS-rust)) |
| Runtime | Custom JVM interpreter in Rust (`jvm/` library crate) |
| Java API | Android-compatible: `picodroid.util.Log`, `picodroid.widget.*` (LVGL-backed UI — incl. `Toast` / `AlertDialog` / `Keyboard`), `picodroid.view.{KeyEvent, GestureDetector, ViewPropertyAnimator}`, `picodroid.graphics.{Theme, drawable.GradientDrawable}`, `picodroid.app.Activity` (full lifecycle + back stack), `picodroid.io` (LittleFS files), `picodroid.content.SharedPreferences`, `picodroid.net` (TCP/UDP + `HttpURLConnection` over WiFi on Pico 2 W), `picodroid.hardware.SensorManager` (BME688), `picodroid.concurrent.Thread` / `Executors`, etc. |
| Logging | [defmt](https://defmt.ferrous-systems.com/) over RTT |

### Architecture

```mermaid
graph TD
    subgraph Hardware
        HW["RP2040 (Cortex-M0+ @ 125 MHz) / RP2350 (Cortex-M33 @ 150 MHz)"]
    end

    subgraph FreeRTOS["FreeRTOS SMP — both cores"]
        PDB["pdb task<br/><i>core 0</i>"]
        JVM_TASK["jvm task + fs/sensor/bg workers<br/><i>core 0</i>"]
        CORE1["flash parker · cyw43 WiFi (Pico 2 W)<br/><i>core 1</i>"]
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

- Raspberry Pi Pico (RP2040), Raspberry Pi Pico 2 (RP2350), or Raspberry Pi Pico 2 W (RP2350 + CYW43439)
- An SWD debug probe: [Raspberry Pi Debug Probe](https://www.raspberrypi.com/products/debug-probe/), Picoprobe, J-Link, or any CMSIS-DAP adapter

On the Pico 2 W the `picodroid.net` stack runs over WiFi end-to-end — WPA2 join, DHCP, TCP/UDP sockets, and HTTP — validated on hardware. See [WiFi & networking setup](https://shivrajora.github.io/picodroid-rs/get-started/networking/) for build-time credentials and the required cyw43 submodule fork.

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

The serial device is `/dev/ttyACM*` on Linux and `/dev/cu.usbmodem*` on macOS.

Display apps (e.g. `displaydemo`) open a graphical window with mouse-as-touch input when run in the simulator.

See [Build & flash](https://shivrajora.github.io/picodroid-rs/get-started/build/) for prerequisites, chip selection, app selection, and UF2 flashing.

## Documentation

The full docs are published at **<https://shivrajora.github.io/picodroid-rs/>** — searchable, cross-linked, and deployed from [`website/`](website/) by [`.github/workflows/docs.yml`](.github/workflows/docs.yml) on every push to `main`. The same content also renders on GitHub directly under [`website/src/content/docs/`](website/src/content/docs/).

**New to Picodroid? Read in this order:** [Build & flash](https://shivrajora.github.io/picodroid-rs/get-started/build/) → [Your first app](https://shivrajora.github.io/picodroid-rs/get-started/first-app/) → [Multi-screen tutorial](https://shivrajora.github.io/picodroid-rs/tutorials/multi-screen-app/) → [Embedded gotchas](https://shivrajora.github.io/picodroid-rs/guides/embedded-gotchas/) → [Limits & memory budgets](https://shivrajora.github.io/picodroid-rs/reference/limits/).

- [Get started → Build & flash](https://shivrajora.github.io/picodroid-rs/get-started/build/) — prerequisites, build, flash, board/app selection
- [Get started → Host simulator](https://shivrajora.github.io/picodroid-rs/get-started/simulator/) — run apps without hardware
- [Get started → Hot-swap with pdb](https://shivrajora.github.io/picodroid-rs/get-started/hot-swap/) — push PAPKs over USB CDC
- [Get started → Your first app](https://shivrajora.github.io/picodroid-rs/get-started/first-app/) — Application/Activity lifecycle and supported language features
- [Tutorials](https://shivrajora.github.io/picodroid-rs/tutorials/multi-screen-app/) — guided builds: a [multi-screen app](https://shivrajora.github.io/picodroid-rs/tutorials/multi-screen-app/) and a [background service](https://shivrajora.github.io/picodroid-rs/tutorials/background-service/)
- [Java API](https://shivrajora.github.io/picodroid-rs/api/) — split by area: [core](https://shivrajora.github.io/picodroid-rs/api/core/), [system](https://shivrajora.github.io/picodroid-rs/api/system/), [services](https://shivrajora.github.io/picodroid-rs/api/services/), [peripherals](https://shivrajora.github.io/picodroid-rs/api/peripherals/), [storage](https://shivrajora.github.io/picodroid-rs/api/storage/), [networking](https://shivrajora.github.io/picodroid-rs/api/networking/), [sensors](https://shivrajora.github.io/picodroid-rs/api/sensors/), [UI](https://shivrajora.github.io/picodroid-rs/api/ui/)
- [Guides](https://shivrajora.github.io/picodroid-rs/guides/embedded-gotchas/) — [embedded gotchas](https://shivrajora.github.io/picodroid-rs/guides/embedded-gotchas/), [button-only navigation](https://shivrajora.github.io/picodroid-rs/guides/button-navigation/), [debugging](https://shivrajora.github.io/picodroid-rs/guides/debugging/), [bundled image assets](https://shivrajora.github.io/picodroid-rs/guides/assets/)
- [Reference](https://shivrajora.github.io/picodroid-rs/reference/limits/) — [limits & memory budgets](https://shivrajora.github.io/picodroid-rs/reference/limits/), the [manifest schema](https://shivrajora.github.io/picodroid-rs/reference/manifest/), the [shrinker](https://shivrajora.github.io/picodroid-rs/reference/shrinker/), the [porting guide](https://shivrajora.github.io/picodroid-rs/reference/porting-guide/)
- [Release notes](https://shivrajora.github.io/picodroid-rs/project/release-notes/) — v0.4.0 → v0.14.0
- [Contributing](CONTRIBUTING.md) — how to contribute, run tests, and add new features

## Project Structure

```text
picodroid-rs/
├── jvm/                # JVM interpreter — reusable library crate (pico-jvm)
│   └── src/            # no_std + alloc only; no hardware dependencies
│
├── picodroid-core/     # Family-neutral framework crate: JVM natives, lifecycle,
│   └── src/            # graphics, networking, drivers, install, host-simulator HAL,
│                       # and porting.rs: the checklist a new MCU family implements
│
├── platforms/
│   └── rp/             # RP-family firmware crate (RP2040 + RP2350)
│       ├── boards/     # Board configs (testbench_rp2040 / _rp2350 / _rp2350w, pico_enviro_mon / _w)
│       ├── mcus/       # Per-MCU linker scripts, FreeRTOS config, heap sizes
│       └── src/        # Boot tasks, RP HAL (hal/rp/ + port/ C shims), pdb transport, flash slot
│
├── sdk/                # Android-compatible Java API (picodroid.*)
│   ├── java/           # Framework Java sources (compiled into firmware Flash)
│   ├── keep.toml       # Shrinker keep list
│   ├── *.tsv           # Generated name lists behind the runtime's c::/m::/d:: constants
│   └── shrink-maps/    # Immutable per-release shrink maps (v<semver>.toml)
│
├── examples/           # Example apps (Java or Kotlin sources + a PicodroidManifest.xml)
│
├── papk-format/        # PAPK container format — single source of truth (firmware + tools)
├── pdb-protocol/       # PDB wire format shared by firmware, simulator, and the pdb CLI
├── compat/             # PAPK ↔ firmware framework-map-version compatibility check
│
├── tools/
│   ├── papk-pack/      # Host tool: packages compiled .class files into a .papk file
│   ├── papk-info/      # Host tool: inspect .papk file contents (manifest, classes, sizes)
│   ├── class-shrink/   # Host tool: class/member-name shrinker — release maps, per-app maps, retrace
│   └── pdb/            # Host tool: push apps, inject input, and monitor device health
│
├── build_support/      # Shared build-script logic (boards, FreeRTOS, network, PAPK embed)
├── scripts/            # Build, flash, sim, pdb, test, HIL, and pre-commit scripts
├── website/            # Astro Starlight documentation site
├── docs/               # Engineering docs: designs, audits, dated bug records
└── third_party/        # All third-party code: submodules (FreeRTOS-Kernel, LVGL, FreeRTOS+TCP, cyw43-driver fork), littlefs fork, formatter JARs
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
