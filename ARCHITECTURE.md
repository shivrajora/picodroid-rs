# Architecture

This document maps the picodroid-rs codebase by **reusability**: which pieces are written to be lifted into another project, which are picodroid-the-application, and where the boundaries between them sit.

For end-user docs (writing apps, porting to a new board, debugging) see the documentation site: <https://shivrajora.github.io/picodroid-rs/>. `docs/` holds engineering notes — designs, audits, and dated bug records.

## Reusable crates

These crates have no picodroid-specific knowledge and could be picked up by a different project as-is. They live under the workspace and are also independently buildable (`cargo build -p <crate>` against a host target).

| Crate | Path | Purpose |
|---|---|---|
| `pico-jvm` | [`jvm/`](jvm/) | `no_std` Java bytecode interpreter. Zero hardware deps, zero container-format knowledge (PAPK parsing lives in `papk-format`). Native methods plug in via the [`NativeMethodHandler`](jvm/src/native/mod.rs) trait. See [`jvm/README.md`](jvm/README.md). |
| `papk-format` | [`papk-format/`](papk-format/) | PAPK container format: `no_std` zero-copy parser + streaming scanners, `alloc`-gated writer (feature `write`). Single source of truth for the on-disk layout; consumed by firmware, build scripts, and every host tool. |
| `compat` | [`compat/`](compat/) | PAPK ↔ firmware version compatibility check. `no_std`. Shared by device + host. See [`compat/README.md`](compat/README.md). |
| `class-shrink` | [`tools/class-shrink/`](tools/class-shrink/) | Build-time Java class/method name shrinker. Host-only (uses `std`). See [`tools/class-shrink/README.md`](tools/class-shrink/README.md). |

Two host-side Gradle projects sit next to the crates and are equally free of picodroid-specific runtime knowledge: [`inject/annotations`](inject/annotations/) (JSR-330 `javax.inject.Inject` / `Singleton` / `Scope`, `SOURCE` retention) and [`inject/compiler`](inject/compiler/) (the javac annotation processor that turns them into plain `Foo_Factory` / `Foo_MembersInjector` classes). `buildSrc`'s `picodroid-papk` plugin wires both into every Java app; the only runtime counterpart is a ~40-line probe in `picodroid-core/src/lifecycle.rs` that calls the generated leaf injector after a component's `<init>`. Design: [`docs/designs/inject-annotations-2026-08.md`](docs/designs/inject-annotations-2026-08.md).

## The picodroid binary

The [`picodroid`](platforms/rp/) crate is an *application* of `pico-jvm` — it is not itself a library. It hosts the JVM on RP2040/RP2350 hardware (or a host simulator), loads framework + app classes, dispatches native methods, drives the display and input, and exposes the developer-facing USB-CDC debugger (`pdb`). Since the 2026-07 extraction it is a thin family shell over [`picodroid-core`](picodroid-core/), which holds the framework itself.

Treat `platforms/rp/` as a **reference implementation** of how to bind a family to `picodroid-core`, not as code to lift wholesale. For porting picodroid to a new MCU, see the [porting guide](https://shivrajora.github.io/picodroid-rs/reference/porting-guide/).

## Module map

The tree split in two during the 2026-07 shared-core extraction. Everything
that does not know what chip it runs on is in `picodroid-core/`; what is left
in `platforms/rp/` is this family and nothing else.

### `picodroid-core/src/` — shared by every family

| Module | Purpose | Tag |
|---|---|---|
| [`boot.rs`](picodroid-core/src/boot.rs) | Shared JVM heap, class loaders, `run_app` | `[picodroid]` |
| [`graphics/`](picodroid-core/src/graphics/) | LVGL engine, widget set, Java-facing binding layer | `[picodroid]` |
| [`native_handler/`](picodroid-core/src/native_handler/) | `pico-jvm` native dispatch (chain-of-responsibility per domain) | `[picodroid]` |
| [`lifecycle.rs`](picodroid-core/src/lifecycle.rs) / [`service_lifecycle.rs`](picodroid-core/src/service_lifecycle.rs) | Activity + Service lifecycle, event dispatch | `[picodroid]` |
| [`hal/`](picodroid-core/src/hal/) | HAL CONTRACT v2 traits, facade, registration macros | `[reusable]` candidate |
| [`hal/sim/`](picodroid-core/src/hal/sim/) | The simulator — shared, not copied per family, incl. the [`allocator.rs`](picodroid-core/src/hal/sim/allocator.rs) device heap cap, [`heap4.rs`](picodroid-core/src/hal/sim/heap4.rs) `heap_4` port and the [`boot_budget.rs`](picodroid-core/src/hal/sim/boot_budget.rs) engine that charges a family's boot model | `[reusable]` candidate |
| [`porting.rs`](picodroid-core/src/porting.rs) | What a port provides: every seam item re-exported, the checklist as its doc, a test that keeps it and the porting guide complete | `[picodroid]` |
| [`rtos/`](picodroid-core/src/rtos/) / [`host.rs`](picodroid-core/src/host.rs) | RTOS trait and platform hooks; `rtos/freertos.rs` is the one FreeRTOS-naming module outside the simulator | `[reusable]` candidate |
| [`executors/`](picodroid-core/src/executors/) | Main queue + background pool, behind the RTOS seam | `[reusable]` candidate |
| [`hardware/`](picodroid-core/src/hardware/) | Sensor sampler and mailbox | `[picodroid]` |
| [`pio/`](picodroid-core/src/pio/) / [`net/`](picodroid-core/src/net/) / [`os/`](picodroid-core/src/os/) / [`util/`](picodroid-core/src/util/) | Java-side peripheral, network, OS and log surface | `[picodroid]` |
| [`gc_roots.rs`](picodroid-core/src/gc_roots.rs) | Root-provider registry (see the GC rule below) | `[picodroid]` |
| [`drivers/`](picodroid-core/src/drivers/) | Peripheral drivers (CYW43, ST7789, XPT2046, BME688, LTR559) | `[hardware]` |
| [`mem_diag.rs`](picodroid-core/src/mem_diag.rs) | Memory monitor | `[picodroid]` |
| [`lvgl_ffi.rs`](picodroid-core/src/lvgl_ffi.rs) | Hand-written LVGL C bindings | `[hardware]` |
| [`shrink_names.rs`](picodroid-core/src/shrink_names.rs) | Runtime class-name un-shrinking | `[picodroid]` |

### `platforms/rp/src/` — this family only

| Module | Purpose | Tag |
|---|---|---|
| [`glue.rs`](platforms/rp/src/glue.rs) | The one file a new family reimplements: HAL / RTOS / hook registration, plus the `register_sim_platform!` call | `[picodroid]` |
| [`main.rs`](platforms/rp/src/main.rs) | Entry point, panic handler, global allocator | `[picodroid]` |
| [`app.rs`](platforms/rp/src/app.rs) | This family's APK blob and post-run idle loop | `[picodroid]` |
| [`boot_tasks.rs`](platforms/rp/src/boot_tasks.rs) | Task topology and the JVM supervisor loop (the reference for the porting guide's checklist) | `[picodroid]` |
| [`boot_budget.rs`](platforms/rp/src/boot_budget.rs) | Stack sizes and the boot-task model the simulator charges | `[picodroid]` |
| [`task_affinity.rs`](platforms/rp/src/task_affinity.rs) | Dual-core placement: the one spawn helper and the scan that enforces it | `[picodroid]` |
| [`hal/rp/`](platforms/rp/src/hal/rp/) | RP2040/RP2350 peripheral implementations | `[hardware]` |
| [`fs/`](platforms/rp/src/fs/) | The flash region's geometry and its `FsBackingStore`; LittleFS itself is core's | `[picodroid]` |
| [`pdb/`](platforms/rp/src/pdb/) | Debug-bridge family glue: CDC transport, park coordinator, FreeRTOS sysmon source. The protocol itself lives in [`picodroid-core/src/pdb/`](picodroid-core/src/pdb/), its wire layouts and USB identity in [`pdb-protocol/`](pdb-protocol/) | `[picodroid]` |
| [`packagemanager/`](platforms/rp/src/packagemanager/) | `PapkSlotFlash` over the flash primitives, and the linker section probe-rs writes; the install orchestration and slot arithmetic are core's | `[picodroid]` |
| [`boards/`](platforms/rp/src/boards/) | Per-board feature glue (memory layout, capability cfgs) | `[picodroid]` |

`[reusable]` candidates are well-layered enough to lift into another project but currently live here because there's only one consumer. If a second consumer materialises, promote them to standalone crates.

## Boundaries that should not be crossed

| Rule | Why |
|---|---|
| `pico-jvm` MUST NOT depend on `cortex_m`, `embassy`, `rp2*`, `cortex_m_rt`, or `panic_*` crates. | The JVM crate's value is that it is hardware-agnostic. Any of these imports would make it Cortex-M-only. Verify with `rg cortex_m jvm/src` (must be empty). |
| `pico-jvm` MUST NOT contain `picodroid/*` class names. | The JVM canonicalises class names via [`BUILTIN_CLASS_NAMES`](jvm/src/native/mod.rs) plus the host-supplied list returned from [`NativeMethodHandler::native_class_names`](jvm/src/native/mod.rs). Picodroid's list lives in [`PICODROID_NATIVE_CLASSES`](picodroid-core/src/native_handler/mod.rs). |
| Adding a new entry to [`BUILTIN_DISPATCH`](jvm/src/native/mod.rs) MUST also add it to `BUILTIN_CLASS_NAMES`. | Without canonicalisation, virtual dispatch silently returns "unknown" and breaks. The `builtin_dispatch_classes_subset_of_names` test enforces this. |
| Adding a new framework class with native methods MUST add its FQN to [`PICODROID_NATIVE_CLASSES`](picodroid-core/src/native_handler/mod.rs). | Same canonicalisation hazard, on the host side. |
| `picodroid-core`'s Java-facing modules are the framework's surface — not a generic library. | Reusing them means you accept the picodroid widget/net/sensor vocabulary. If you want only the JVM, depend on `pico-jvm` directly. |
| A family's `hal/` MUST NOT import from the framework modules. | HAL is a leaf. Verify with `rg "use crate::(system\|app)" platforms/*/src/hal/` (must be empty). |
| A platform crate MUST NOT construct a `Jvm` — hand off to [`boot::run_app`](picodroid-core/src/boot.rs). | Two JVM-driving crates monomorphise the interpreter twice: measured at ~38 KB across 23 duplicated symbols, which overflowed the RP2040 flash ceiling. LTO does not rescue it. |
| A file MUST NOT exist at the same relative path under both `platforms/*/src` and `picodroid-core/src`. | Shadow twins compile, one copy goes dead, and they drift silently (commit `fc896b3`; ESP's removed scaffold had 17). Enforced by `scripts/pre-commit`, with an allowlist for the four genuine seam pairs (`gc_root_registration.rs`, `hal/mod.rs`, `pdb/mod.rs`, `fs/mod.rs`). |
| Every task the RP family creates MUST go through `task_affinity::spawn`, naming its core as `task_affinity::CORE0` — or `CORE1` only for a task listed in `CORE1_TASKS`; nothing else may write `Task::new()`, `.core_affinity(`, `xTaskCreate*` or `vTaskCoreAffinitySet`. | The shared JVM heap is lock-free on "one core interprets Java"; `volatile` is ignored and no barriers are emitted. `spawn` makes create+pin one scheduler-atomic step (freertos-rust pins *after* `xTaskCreate`, and the SMP kernel would otherwise start the task on idle core 1 first). Enforced by the source scan in [task_affinity.rs](platforms/rp/src/task_affinity.rs), which runs under `scripts/test.sh`. |
| A native module holding Java object references MUST register a root provider. | An unregistered provider is swept while live and fails silently much later as dead input or `NoSuchMethod`. Both crates carry a source-scanning completeness guard over `gc_root_registration.rs`. |
| Wire formats and wire identities live in the small protocol crates ([`pdb-protocol`](pdb-protocol/), [`papk-format`](papk-format/)) and are never hand-mirrored. | A copy on each end drifts, and here a drift fails as a corrupt install or an undetected device rather than loudly. The frame constants, the payload layouts, the boot-meta sector and the USB VID/PID have each been a hand-mirrored pair at some point. |
| Simulator policy a family owns crosses as `register_sim_platform!` parameters (GC roots, the boot-budget model, the app entry), never as `PlatformHooks` methods. | A hook every *device* family must also stub exports simulator data to shared code for no reason; a macro parameter is a decision the family writes down once. |
| Shared code reaches the kernel only through [`picodroid_core::rtos`](picodroid-core/src/rtos/mod.rs). | Otherwise a second family cannot register its own kernel. Enforced by `rtos::seam_guard`, a source scan that bans FreeRTOS API names outside the simulator's backing and `rtos/freertos.rs`. |

## Multi-family seams

Picodroid runs on RP2040 / RP2350 today, under `platforms/rp/`, with cross-family shared code in `picodroid-core/`. An ESP32-S3 (Lilygo T-Deck Plus) Milestone-1 scaffold was removed in 2026-07 — retrieve it from git history (`platforms/esp/`) if a second family returns. The seams below remain the contract for future ports; [`picodroid_core::porting`](picodroid-core/src/porting.rs) re-exports every one of them and is the checklist a port works from.

### Family routing

A family is its own binary crate under `platforms/<name>/`, depending on `picodroid-core`. Its `src/hal/mod.rs` routes a single `mod chip;` between its own peripherals and the shared simulator in [`picodroid_core::hal::sim`](picodroid-core/src/hal/sim/). Nothing in `picodroid-core` needs editing to add one.

### HAL CONTRACT v2

The contract is the trait set in [`picodroid_core::hal`](picodroid-core/src/hal/traits.rs): `HalDisplay`, `HalGpio`, `HalClock`, `HalTouch`, `HalI2c`, `HalAdc`, `HalPwm`, `HalSpi`, `HalUart`, `HalFs`, and `HalNet` under `cfg(has_network)`. A family implements them and calls `set_hal!`, which emits the `#[no_mangle]` shims that core's facade binds at link time. A drifted signature fails to compile at the impl.

This replaced v1's hand-written doc-block plus matching assertion list. The two had fallen out of step: converting to traits found `net::udp_sendto`/`udp_recvfrom` and `i2c::{write,read}` / `spi::{transfer,write}` / `uart::reconfigure` in live use by the natives and named in neither half.

- **Trait-covered**: everything above, plus the debug bridge and install path — `PdbTransport`, `SysmonSource`, `CoreCoordinator` and `PapkFlash` in [`picodroid_core::pdb`](picodroid-core/src/pdb/mod.rs) / [`picodroid_core::install`](picodroid-core/src/install/orchestrator.rs) replaced the former `pdb_usb::*` assertion block; a family normally implements `PapkSlotFlash` (three constants and two flash primitives) and lets [`PapkSlot`](picodroid-core/src/install/slot.rs) be its `PapkFlash`. The filesystem is `FsBackingStore` behind core's `littlefs` feature.
- **Assertion-covered** (no trait, no shared counterpart): `boot::clock_init`, `flash::read_flash_papk` — still checked by [contract.rs](platforms/rp/src/hal/contract.rs). (`boot::start_tasks` left for `boot_tasks.rs` in stage 3f.)
- **Family-private**: `delay`, `input_pin`, `output_pin`, `spi_bus` wire `picodroid-core`'s generic drivers to a family's peripherals; name and shape are the family's own.

Chip-within-family symbols (e.g. `pdb_usb::queue_read_byte_busywait`, RP2350-only) are conditionally compiled at the family-internal level.

### MCU TOML schema

[platforms/&lt;family&gt;/mcus/&lt;family&gt;/&lt;mcu&gt;.toml](platforms/rp/mcus/) drives the build. [build_support/freertos.rs](build_support/freertos.rs) consumes:

- `freertos_port` — kernel port path
- `pico_shim` — extra C source compiled with the kernel
- `freertos_port_extra_includes` — semicolon-separated C include paths
- `freertos_c_defines` — semicolon-separated `KEY=VALUE` defines
- `freertos_vector_aliases` — semicolon-separated `CMSIS=portasm` linker aliases
- `init_array_segment` — destination memory region for `.init_array` (RP-specific quirk; leave unset on platforms that don't need it)

[build_support/network.rs](build_support/network.rs) takes `mcu_family` and reads `platforms/<family>/src/hal/<family>/port` for the network glue. Today network is CYW43+FreeRTOS+TCP and only ships on RP; a future family using esp-idf/lwIP should add a parallel network module rather than extending this one.

### Naming convention

- `family-<name>` (Cargo feature) — e.g. `family-rp`. Activated transitively by chip features.
- `chip-<mcu_name>` (Cargo feature) — e.g. `chip-rp2040`, `chip-rp2350`. Mechanical 1:1 with `platforms/<family>/mcus/<family>/<mcu_name>.toml`.
- `board-<board_name>` (Cargo feature) — e.g. `board-testbench-rp2040`. Mechanical 1:1 with `platforms/<family>/boards/<board_name>/`.

Boards declare their MCU via `mcu = "..."` in `board.toml`; [build_support/config.rs](build_support/config.rs)::`resolve_active_mcu` reads it directly. Chip features only exist to gate dep crates.

### RP-specific concerns kept isolated for any future family

The following are deeply RP-specific and live entirely under [platforms/rp/src/hal/rp/](platforms/rp/src/hal/rp/). When a second hardware family is added, equivalent mechanisms (or replacements) will be derived for that family — the refactor's job was just to keep them isolated, not to abstract them.

- **SMP / cross-core FIFO / Amazon-SMP affinity APIs** — [platforms/rp/src/boot_tasks.rs](platforms/rp/src/boot_tasks.rs) and the spawn arms in [glue.rs](platforms/rp/src/glue.rs) create every task through `task_affinity::spawn` with a V11 SMP core-affinity mask spelled as `task_affinity::CORE0` / `CORE1` ([task_affinity.rs](platforms/rp/src/task_affinity.rs)), which suspends the scheduler around create+pin. `configTASK_DEFAULT_CORE_AFFINITY` / `configIDLE_AFFINITY` are deliberately left unset: they pin the idle-task reaper (see `FreeRTOSConfig.h`). ESP-IDF FreeRTOS uses `xTaskCreatePinnedToCore` and stack sizes in bytes (not words).
- **Same-core install flow** — PDB and JVM tasks are both pinned to core 0 (an RP2350 cross-core SRAM visibility bug retired the original cross-core park design); during install the JVM blocks on a FreeRTOS notification, and each flash erase/program window disables interrupts inside `with_xip_disabled!` ([platforms/rp/src/hal/rp/flash.rs](platforms/rp/src/hal/rp/flash.rs)). ESP32-S3 has cache-suspension APIs (`esp_flash_suspend_cache`) that obviate this pattern.
- **`platforms/rp/mcus/rp/FreeRTOSConfig.h` ARM macros** — keyed off `__ARM_ARCH_8M_MAIN__`. A future family supplies its own config keyed to its architecture.
