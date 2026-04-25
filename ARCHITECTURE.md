# Architecture

This document maps the picodroid-rs codebase by **reusability**: which pieces are written to be lifted into another project, which are picodroid-the-application, and where the boundaries between them sit.

For end-user docs (writing apps, porting to a new board, debugging) see [`docs/`](docs/).

## Reusable crates

These crates have no picodroid-specific knowledge and could be picked up by a different project as-is. They live under the workspace and are also independently buildable (`cargo build -p <crate>` against a host target).

| Crate | Path | Purpose |
|---|---|---|
| `pico-jvm` | [`jvm/`](jvm/) | `no_std` Java bytecode interpreter. Zero hardware deps. Native methods plug in via the [`NativeMethodHandler`](jvm/src/native/mod.rs) trait. See [`jvm/README.md`](jvm/README.md). |
| `compat` | [`compat/`](compat/) | PAPK ↔ firmware version compatibility check. `no_std`. Shared by device + host. See [`compat/README.md`](compat/README.md). |
| `class-shrink` | [`tools/class-shrink/`](tools/class-shrink/) | Build-time Java class/method name shrinker. Host-only (uses `std`). See [`tools/class-shrink/README.md`](tools/class-shrink/README.md). |

## The picodroid binary

The [`picodroid`](src/) crate is an *application* of `pico-jvm` — it is not itself a library. It hosts the JVM on RP2040/RP2350 hardware (or a host simulator), loads framework + app classes, dispatches native methods, drives the display and input, and exposes the developer-facing UART debugger (`pdb`).

Treat `src/` as a **reference implementation** of how to embed `pico-jvm` on Cortex-M, not as code to lift wholesale into another project. For porting picodroid to a new board, see [`docs/porting-guide.md`](docs/porting-guide.md).

## Module map (`src/`)

| Module | Purpose | Tag |
|---|---|---|
| [`app.rs`](src/app.rs) | JVM bootstrap, class loading, app lifecycle entry | `[picodroid]` |
| [`main.rs`](src/main.rs) | FreeRTOS init, hardware bringup | `[picodroid]` |
| [`boards/`](src/boards/) | Per-board feature glue (memory layout, capability cfgs) | `[picodroid]` |
| [`drivers/`](src/drivers/) | Peripheral drivers (CYW43 WiFi, ST7789 LCD, XPT2046 touch) | `[hardware]` |
| [`fs/`](src/fs/) | LittleFS wrapper (single- and multi-threaded variants) | `[reusable]` candidate |
| [`hal/`](src/hal/) | HAL with `rp/` (RP2040/RP2350) and `sim/` chip dispatchers | `[reusable]` candidate |
| [`lifecycle.rs`](src/lifecycle.rs) | Widget event dispatch, app suspend/resume | `[picodroid]` |
| [`lvgl_ffi.rs`](src/lvgl_ffi.rs) | Hand-written LVGL C bindings | `[hardware]` |
| [`packagemanager/`](src/packagemanager/) | Hot-reload PAPK install over USB | `[picodroid]` |
| [`pdb/`](src/pdb/) | Picodroid Debug Bridge (USB-CDC protocol + sysmon) | `[picodroid]` |
| [`shrink_names.rs`](src/shrink_names.rs) | Runtime class-name un-shrinking | `[picodroid]` |
| [`sim_allocator.rs`](src/sim_allocator.rs) | Heap limiter for host simulator | `[picodroid]` |
| [`system/executors/`](src/system/executors/) | FreeRTOS task wrappers (main queue + background pool) | `[reusable]` candidate |
| [`system/monitor_store/`](src/system/monitor_store/) | Reentrant monitor store backing Java `synchronized` | `[reusable]` candidate |
| [`system/native_handler/`](src/system/native_handler/) | `pico-jvm` native dispatch (chain-of-responsibility per domain) | `[picodroid]` |
| [`system/picodroid/`](src/system/picodroid/) | Java-side framework surface (widgets, net, sensors, PIO) | `[picodroid]` |

`[reusable]` candidates are well-layered enough to lift into another project but currently live in `src/` because there's only one consumer. If a second consumer materialises, promote them to standalone crates.

## Boundaries that should not be crossed

| Rule | Why |
|---|---|
| `pico-jvm` MUST NOT depend on `cortex_m`, `embassy`, `rp2*`, `cortex_m_rt`, or `panic_*` crates. | The JVM crate's value is that it is hardware-agnostic. Any of these imports would make it Cortex-M-only. Verify with `rg cortex_m jvm/src` (must be empty). |
| `pico-jvm` MUST NOT contain `picodroid/*` class names. | The JVM canonicalises class names via [`BUILTIN_CLASS_NAMES`](jvm/src/native/mod.rs) plus the host-supplied list returned from [`NativeMethodHandler::native_class_names`](jvm/src/native/mod.rs). Picodroid's list lives in [`PICODROID_NATIVE_CLASSES`](src/system/native_handler/mod.rs). |
| Adding a new entry to [`BUILTIN_DISPATCH`](jvm/src/native/mod.rs) MUST also add it to `BUILTIN_CLASS_NAMES`. | Without canonicalisation, virtual dispatch silently returns "unknown" and breaks. The `builtin_dispatch_classes_subset_of_names` test enforces this. |
| Adding a new framework class with native methods MUST add its FQN to [`PICODROID_NATIVE_CLASSES`](src/system/native_handler/mod.rs). | Same canonicalisation hazard, on the host side. |
| `src/system/picodroid/` is the framework's Java-side surface — not a generic library. | Reusing it means you accept the picodroid widget/net/sensor vocabulary. If you want only the JVM, depend on `pico-jvm` directly. |
| `src/hal/` MUST NOT import from `src/system/` or `src/app/`. | HAL is a leaf. Verify with `rg "use crate::(system|app)" src/hal/` (must be empty). |
