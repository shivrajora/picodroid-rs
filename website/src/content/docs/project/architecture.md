---
title: "Architecture"
description: "Module layout, HAL contract, and multi-family seams."
---

This document maps the picodroid-rs codebase by **reusability**: which pieces are written to be lifted into another project, which are picodroid-the-application, and where the boundaries between them sit.

For end-user docs (writing apps, porting to a new board, debugging) see [the website](/).

## Reusable crates

These crates have no picodroid-specific knowledge and could be picked up by a different project as-is. They live under the workspace and are also independently buildable (`cargo build -p <crate>` against a host target).

| Crate | Path | Purpose |
|---|---|---|
| `pico-jvm` | [`jvm/`](https://github.com/shivrajora/picodroid-rs/tree/main/jvm/) | `no_std` Java bytecode interpreter. Zero hardware deps. Native methods plug in via the [`NativeMethodHandler`](https://github.com/shivrajora/picodroid-rs/blob/main/jvm/src/native/mod.rs) trait. See [`jvm/README.md`](https://github.com/shivrajora/picodroid-rs/tree/main/jvm/README.md). |
| `compat` | [`compat/`](https://github.com/shivrajora/picodroid-rs/tree/main/compat/) | PAPK ↔ firmware version compatibility check. `no_std`. Shared by device + host. See [`compat/README.md`](https://github.com/shivrajora/picodroid-rs/tree/main/compat/README.md). |
| `class-shrink` | [`tools/class-shrink/`](https://github.com/shivrajora/picodroid-rs/tree/main/tools/class-shrink/) | Build-time Java class/method name shrinker. Host-only (uses `std`). See [`tools/class-shrink/README.md`](https://github.com/shivrajora/picodroid-rs/tree/main/tools/class-shrink/README.md). |

## The picodroid binary

The [`picodroid`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/) crate is an *application* of `pico-jvm` — it is not itself a library. It hosts the JVM on RP2040/RP2350 hardware (or a host simulator), loads framework + app classes, dispatches native methods, drives the display and input, and exposes the developer-facing USB-CDC debugger (`pdb`).

Treat `platforms/rp/src/` as a **reference implementation** of how to embed `pico-jvm` on Cortex-M, not as code to lift wholesale into another project. For porting picodroid to a new board, see [`docs/porting-guide.md`](/reference/porting-guide/).

## Module map (`platforms/rp/src/`)

| Module | Purpose | Tag |
|---|---|---|
| [`app.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/app.rs) | JVM bootstrap, class loading, app lifecycle entry | `[picodroid]` |
| [`main.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/main.rs) | FreeRTOS init, hardware bringup | `[picodroid]` |
| [`boards/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/boards/) | Per-board feature glue (memory layout, capability cfgs) | `[picodroid]` |
| [`drivers/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/drivers/) | Peripheral drivers (CYW43 WiFi, ST7789 LCD, XPT2046 touch) | `[hardware]` |
| [`fs/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/fs/) | LittleFS wrapper (single- and multi-threaded variants) | `[reusable]` candidate |
| [`hal/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/hal/) | HAL with `rp/` (RP2040/RP2350) and `sim/` chip dispatchers | `[reusable]` candidate |
| [`lifecycle.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/lifecycle.rs) | Widget event dispatch, app suspend/resume | `[picodroid]` |
| [`lvgl_ffi.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/lvgl_ffi.rs) | Hand-written LVGL C bindings | `[hardware]` |
| [`packagemanager/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/packagemanager/) | Hot-reload PAPK install over USB | `[picodroid]` |
| [`pdb/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/pdb/) | Picodroid Debug Bridge (USB-CDC protocol + sysmon) | `[picodroid]` |
| [`shrink_names.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/shrink_names.rs) | Runtime class-name un-shrinking | `[picodroid]` |
| [`sim_allocator.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/sim_allocator.rs) | Heap limiter for host simulator | `[picodroid]` |
| [`system/executors/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/system/executors/) | FreeRTOS task wrappers (main queue + background pool) | `[reusable]` candidate |
| [`system/monitor_store.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/system/monitor_store.rs) | Reentrant monitor store backing Java `synchronized` | `[reusable]` candidate |
| [`system/native_handler/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/system/native_handler/) | `pico-jvm` native dispatch (chain-of-responsibility per domain) | `[picodroid]` |
| [`system/picodroid/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/system/picodroid/) | Java-side framework surface (widgets, net, sensors, PIO) | `[picodroid]` |

`[reusable]` candidates are well-layered enough to lift into another project but currently live in `platforms/rp/src/` because there's only one consumer. If a second consumer materialises, promote them to standalone crates.

## Boundaries that should not be crossed

| Rule | Why |
|---|---|
| `pico-jvm` MUST NOT depend on `cortex_m`, `embassy`, `rp2*`, `cortex_m_rt`, or `panic_*` crates. | The JVM crate's value is that it is hardware-agnostic. Any of these imports would make it Cortex-M-only. Verify with `rg cortex_m jvm/src` (must be empty). |
| `pico-jvm` MUST NOT contain `picodroid/*` class names. | The JVM canonicalises class names via [`BUILTIN_CLASS_NAMES`](https://github.com/shivrajora/picodroid-rs/blob/main/jvm/src/native/mod.rs) plus the host-supplied list returned from [`NativeMethodHandler::native_class_names`](https://github.com/shivrajora/picodroid-rs/blob/main/jvm/src/native/mod.rs). Picodroid's list lives in [`PICODROID_NATIVE_CLASSES`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/system/native_handler/mod.rs). |
| Adding a new entry to [`BUILTIN_DISPATCH`](https://github.com/shivrajora/picodroid-rs/blob/main/jvm/src/native/mod.rs) MUST also add it to `BUILTIN_CLASS_NAMES`. | Without canonicalisation, virtual dispatch silently returns "unknown" and breaks. The `builtin_dispatch_classes_subset_of_names` test enforces this. |
| Adding a new framework class with native methods MUST add its FQN to [`PICODROID_NATIVE_CLASSES`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/system/native_handler/mod.rs). | Same canonicalisation hazard, on the host side. |
| `platforms/rp/src/system/picodroid/` is the framework's Java-side surface — not a generic library. | Reusing it means you accept the picodroid widget/net/sensor vocabulary. If you want only the JVM, depend on `pico-jvm` directly. |
| `platforms/rp/src/hal/` MUST NOT import from `system/` or `app/`. | HAL is a leaf. Verify with `rg "use crate::(system|app)" platforms/rp/src/hal/` (must be empty). |

## Multi-family seams

Picodroid runs on RP2040/RP2350 today, with ESP32-S3 (Lilygo T-Deck Plus) **Milestone 1 scaffolding** landed under `platforms/esp/` (compile-only so far). The codebase is structured so that adding a chip family is additive rather than touching dozens of files. The seams below are the contract for ports.

### Family routing

[platforms/rp/src/hal/mod.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/mod.rs) dispatches a single `mod chip;` to the active family via `cfg(feature = "family-<name>")`. Sim/test always routes to `platforms/rp/src/hal/sim/`. Add a new family by creating `platforms/<name>/src/hal/` exposing the symbols listed in **HAL CONTRACT v1** (see the doc-block at the top of [platforms/rp/src/hal/mod.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/mod.rs)) and adding the `family-<name>` feature to [Cargo.toml](https://github.com/shivrajora/picodroid-rs/blob/main/Cargo.toml).

### HAL CONTRACT v1

The required public symbols and signatures are documented in [platforms/rp/src/hal/mod.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/mod.rs); they are enforced at compile time by [platforms/rp/src/hal/contract.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/contract.rs). Drift in any signature breaks `cargo clippy` (rp2040, rp2350) and `cargo test --no-run` (sim). Symbols are tiered:

- **Always required** (sim + every hardware family): `adc`, `display`, `gpio`, `i2c`, `pwm`, `spi`, `system_clock`, `touch`, `uart`.
- **Hardware-only** (gated by `not(any(test, feature = "sim"))`): `boot::clock_init`, `boot::start_tasks`, `flash::read_flash_papk`, `pdb_usb::*`.
- **Network-only** (gated by `cfg(has_network)`): `net::*`.

Chip-within-family symbols (e.g. `pdb_usb::queue_read_byte_busywait`, RP2350-only) are not part of the cross-family contract — they're conditionally compiled at the family-internal level.

### MCU TOML schema

[platforms/&lt;family&gt;/mcus/&lt;family&gt;/&lt;mcu&gt;.toml](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/mcus/) drives the build. [build_support/freertos.rs](https://github.com/shivrajora/picodroid-rs/blob/main/build_support/freertos.rs) consumes:

- `freertos_port` — kernel port path
- `pico_shim` — extra C source compiled with the kernel
- `freertos_port_extra_includes` — semicolon-separated C include paths
- `freertos_c_defines` — semicolon-separated `KEY=VALUE` defines
- `freertos_vector_aliases` — semicolon-separated `CMSIS=portasm` linker aliases
- `init_array_segment` — destination memory region for `.init_array` (RP-specific quirk; leave unset on platforms that don't need it)

[build_support/network.rs](https://github.com/shivrajora/picodroid-rs/blob/main/build_support/network.rs) takes `mcu_family` and reads `platforms/<family>/src/hal/<family>/port` for the network glue. Today network is CYW43+FreeRTOS+TCP and only ships on RP; a future family using esp-idf/lwIP should add a parallel network module rather than extending this one.

### Naming convention

- `family-<name>` (Cargo feature) — e.g. `family-rp`. Activated transitively by chip features.
- `chip-<mcu_name>` (Cargo feature) — e.g. `chip-rp2040`, `chip-rp2350`. Mechanical 1:1 with `platforms/<family>/mcus/<family>/<mcu_name>.toml`.
- `board-<board_name>` (Cargo feature) — e.g. `board-testbench-rp2040`. Mechanical 1:1 with `boards/<board_name>/`.

Boards declare their MCU via `mcu = "..."` in `board.toml`; [build_support/config.rs](https://github.com/shivrajora/picodroid-rs/blob/main/build_support/config.rs)::`resolve_active_mcu` reads it directly. Chip features only exist to gate dep crates.

### RP-specific patterns (boot, flash, timer)

The following are deeply RP-specific and live entirely under [`platforms/rp/src/hal/rp/`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/src/hal/rp/). As hardware families are added (ESP32-S3 scaffolding has begun under `platforms/esp/`), equivalent mechanisms (or replacements) are derived per family — the refactor's job was just to keep them isolated, not to abstract them.

- **SMP / cross-core FIFO / Amazon-SMP affinity APIs** — [src/hal/rp/boot.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/rp/boot.rs) uses `xTaskCreateAffinitySet` (V11 SMP). ESP-IDF FreeRTOS uses `xTaskCreatePinnedToCore` and stack sizes in bytes (not words).
- **Park-for-flash dance** — [src/hal/rp/flash.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/rp/flash.rs) parks core 0 with interrupts disabled while core 1 erases flash. ESP32-S3 has cache-suspension APIs (`esp_flash_suspend_cache`) that obviate this pattern.
- **RP2350 cross-core timer alarm** — [src/hal/rp/timer_alarm.rs](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/rp/timer_alarm.rs) compensates for the RP2350 tick freezing during park-for-flash. Different chip, different mechanism.
- **`platforms/rp/mcus/rp/FreeRTOSConfig.h` ARM macros** — keyed off `__ARM_ARCH_8M_MAIN__`. A `platforms/esp/mcus/esp/FreeRTOSConfig.h` already exists as an M1 placeholder and will gain its Xtensa-aware settings when FreeRTOS-on-Xtensa is implemented.
