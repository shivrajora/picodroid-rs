---
title: "Porting Guide: Adding a New MCU to picodroid"
description: "What a new MCU family provides to picodroid-core, and where each obligation is checked."
---

This guide explains how picodroid is split between shared code and a chip
family, and everything a new family has to provide. The list below is also
in the code: `picodroid_core::porting` re-exports every item a port
implements, and a test fails if that module or this page stops naming one.

## Architecture overview

Chip-specific code lives under `platforms/<family>/`, one cargo crate per
family. Everything that does not know what chip it is running on — the JVM
natives, the widget set, the LVGL engine, the lifecycle, the simulator, the
debug bridge protocol, the installer, the filesystem — lives in
`picodroid-core/`. The boundary between them is a set of Rust traits and
macros a family implements, bound at link time.

```text
picodroid-core/           # everything shared, including the simulator
  src/porting.rs          # the checklist: re-exports every seam item
  src/hal/                # HAL CONTRACT v2 traits, facade, set_hal_*! macros
  src/hal/sim/            # the simulator (shared, never copied per family)
  src/rtos/               # the Rtos trait; rtos/freertos.rs for FreeRTOS families
  src/host.rs             # PlatformHooks
  src/pdb/, src/install/  # debug bridge and installer, generic over your impls
  src/fs/                 # LittleFS behind the `littlefs` feature
platforms/
  rp/                     # RP2040 + RP2350: the reference implementation
    boards/<board>/board.toml
    mcus/rp/<chip>.toml, FreeRTOSConfig.h, linker scripts
    src/glue.rs           # the one file that binds core's seams to this family
    src/hal/mod.rs        # routes `mod chip` between rp/ and the shared simulator
    src/hal/contract.rs   # assertions for the two things no trait covers (boot, flash)
    src/hal/rp/           # this family's peripheral drivers
    src/boot_tasks.rs     # task topology and the JVM supervisor loop
  <your-family>/          # your new MCU family, same shape
```

### Family vs. board

The family crate exposes chip-level capability. Per-board configuration —
display controller, touch controller, buttons, sensors, network — lives in
`boards/<board>/board.toml` and `mcus/<family>/<chip>.toml`. A new board on
an already-supported family needs only new TOML entries.

## What a port provides

Seven kinds of thing, and no edits to `picodroid-core`. Each is described
in its own section below; `picodroid_core::porting` is the canonical list.

1. **The hardware layer** — trait impls plus a registration macro.
2. **The kernel** — an `Rtos` impl plus `set_rtos!`.
3. **The hooks** — a `PlatformHooks` impl plus `set_platform_hooks!`, and a
   GC-root registration file.
4. **The filesystem** — a backing store for core's LittleFS, or your own
   `HalFs`.
5. **The debug bridge and installer** (optional) — four small traits handed
   to `run_pdb_task`.
6. **The simulator** — one `register_sim_platform!` call and one
   `declare_sim_global_allocator!` call.
7. **Boot, data and discipline** — a `main.rs`, a `build.rs`, `board.toml`,
   and a JVM supervisor loop that meets a short checklist.

You get the simulator, the widget set, the LVGL engine, the lifecycle, the
JVM natives, the debug bridge protocol, the installer, sensor plumbing and
every registry guard for free.

### Do not copy `hal/sim/`

The simulator lives in `picodroid-core/src/hal/sim/` and is shared by every
family. Your `src/hal/mod.rs` routes to it in simulator and test builds (see
[HAL dispatch](#hal-dispatch)); you write no simulator code. An earlier
revision of this guide said to copy it; the ESP32-S3 scaffold did, grew
seventeen stub modules that drifted from the originals, and was removed.
`scripts/pre-commit` now fails if any path exists under both
`platforms/*/src` and `picodroid-core/src`, apart from four allowlisted seam
pairs (`gc_root_registration.rs`, `hal/mod.rs`, `pdb/mod.rs`, `fs/mod.rs`),
each of which is one name for two ends of one seam, not a copy.

### Your crate must not drive the JVM

Hand off to `picodroid_core::boot::run_app(apk_data)` and let core own every
`Jvm`. A family crate that constructs its own `Jvm` gets the whole
interpreter monomorphised a second time — measured at ~38 KB, which
overflowed the RP2040 flash ceiling. LTO does not rescue this.

## 1. The hardware layer (HAL CONTRACT v2)

The contract is the trait set in `picodroid_core::hal`. Implement each for
one type and register with `set_hal!`, which emits the `#[no_mangle]` shims
core's facade binds to; a signature that drifts fails to compile at your
impl. The RP family implements them in `glue.rs` by delegating to free
functions in `hal/rp/`, but that is a convention, not a requirement — the
traits are the whole contract.

| Trait | Methods | Notes |
|---|---|---|
| `HalDisplay` | 8 | `init`, window and pixel push, backlight, sleep/wake, `update_window`, `is_window_open`. Geometry comes from `board_cfg::display`, not from you. |
| `HalGpio` | 11 | Direction, value, `set_input(pin, Pull)`, `read`, edge IRQ enable/disable, `init_gpio_irq`, `inject`, `drain_gpio_event`, `has_pending_event`, `wait_for_button_event`. |
| `HalClock` | 2 | `sleep(ms)` and `elapsed_realtime_nanos()`. Do **not** put a debug-stop check in `sleep`; shared code owns it. |
| `HalTouch` | 7 | `init`, `read_point`, `read_raw_unfiltered`, `set_calibration`, and the three scripted-touch overrides. |
| `HalI2c` | 4 + 2 defaulted | `init`, `set_speed`, `write_slice`, `read_slice`. The Java-array `write`/`read` are defaulted over the slice pair. |
| `HalAdc` | 2 | `init(pin)`, `read(pin) -> volts`. |
| `HalPwm` | 2 | `init(pin)`, `apply(pin, freq_hz, duty_pct, enabled)`. |
| `HalSpi` | 4 + 2 defaulted | `init`, `reconfigure`, `write_raw`, `transfer_raw`. The Java-array `transfer`/`write` are defaulted. |
| `HalUart` | 4 | `init`, `write_byte`, `read_byte` (-1 when empty), `reconfigure(baud, data, parity, stop, flow)`. |
| `HalNet` | 14 | Only on a board with `has_network = true`; registered with `set_hal_net!`, not `set_hal!`. See [The network](#the-network). |
| `HalFs` | 10 | Registered with `set_hal_fs!`. Usually `set_hal_fs!(picodroid_core::fs::LittleFsHal)` — see [The filesystem](#4-the-filesystem). |

`set_hal!` takes the nine bus and display traits at once; there are also
per-trait forms (`set_hal_display!`, `set_hal_gpio!`, `set_hal_clock!`,
`set_hal_touch!`, `set_hal_i2c!`, `set_hal_adc!`, `set_hal_pwm!`,
`set_hal_spi!`, `set_hal_uart!`) for a family that registers them in
separate files. Exactly one registration of each may be linked: your device
and simulator arms must be `cfg`-exclusive.

Types that cross the seam live in `picodroid_core::hal::types` and are
re-exported from `porting`: `Pull { None, Up, Down }`, `EdgeTrigger
{ Rising, Falling, Both }`, `GpioEvent { pin, rising, t_us }` and
`NetError`. Stamp `t_us` at **enqueue** time (in the interrupt): the contact
debounce compares enqueue-time deltas, because edges can sit in the queue
for hundreds of milliseconds while the UI task is busy.

Two building blocks are shared so you do not write them:

- **`GpioEventRing<N>`** — the queue between your GPIO interrupt and the UI
  task. One producer (the interrupt, or a task that has masked it), one
  consumer. Stamp the time and give your wake semaphore beside it; the ring
  keeps the drop tally and warns on overflow. The simulator uses the same
  ring behind a mutex.
- **`TouchOverride`** — the scripted-touch state machine `pdb input tap`
  drives. Keep one as a `static`, forward the three `HalTouch` override
  methods to it, and `match` its `sample()` in `read_point`.

The Java-array bus methods are defaulted through `hal::array_io`, which
stages up to 64 bytes on the stack. A family whose bus takes more overrides
the methods, not the constant.

## 2. The kernel (`Rtos`)

Implement `picodroid_core::rtos::Rtos` (an `unsafe trait`: it promises real
mutual exclusion and real cross-task wake-ups) and register it with
`set_rtos!`. Its 23 methods, grouped:

- `spawn(&TaskSpec, body)` — a task may be **declined** (return `false`);
  the framework copes.
- `queue_create/send/recv` (a `u32` word) and the pointer-width triple
  `queue_create_ptr/send_ptr/recv_ptr`.
- `task_current`, `task_notify`, `task_wait_notification`, and
  `scheduler_running`.
- `mutex_recursive_create/lock/unlock/delete` — Java monitors re-enter.
- `sem_binary_create/give/take`.
- `tick_timer_start/pause/resume/stop` — the UI tick; `pause` must really
  quiesce the timer so the chip can idle.
- `delay_ms`.

Rules that are easy to get wrong:

- **Stack sizes are bytes.** FreeRTOS counts words; ESP-IDF counts bytes.
  Convert in your impl, once.
- **`TaskKind` is your policy hook.** Shared code says what a task is for —
  `Jvm`, `JvmChild`, `BgWorker`, `Sensor`, `FsWorker` — and you decide its
  stack, core and any bookkeeping. On a family that runs from the flash it
  writes, pin `FsWorker` to the core that does the writing: a dual-core
  device corrupts only when a flash write races an instruction fetch on the
  other core, and the simulator can never show it.
- **`scheduler_running` is a real scheduler-state query.** Not
  `task_current() != 0`: FreeRTOS assigns a current task at the first task
  *creation*, so that spelling reports "running" during boot, and a caller
  that trusts it blocks on a notification no running task can send — a boot
  hang on the device that passes every host test.
- **A `JvmChild` is counted before it is created**, and registers itself
  as its first act, if you track children for a debug bridge. A child at a
  higher priority than its parent can finish before `spawn` returns.
- Priorities come from `picodroid_core::task_priority`. The background pool
  runs on the JVM's tier by design (`PRIORITY_JVM_NORM`, 15).
- On FreeRTOS, call `picodroid_core::rtos::freertos::install_heap_atomic_hooks()`
  before the first task exists. It makes the JVM's heap compounds and the
  GC scheduler-atomic; the simulator's boot calls the same function.

## 3. The hooks (`PlatformHooks`)

Implement `picodroid_core::host::PlatformHooks` and register it with
`set_platform_hooks!`. Six methods: `stop_requested` (has a debugger asked
the JVM to stop — `false` if you have no bridge), `heap_bypass_enter/exit`
and `heap_checkpoint` (no-ops on hardware; the simulator's heap model uses
them), `native_heap_stats` (what the memory monitor prints), and
`register_gc_roots`.

`register_gc_roots` is required, not defaulted, on purpose: a family with no
native module holding Java references writes an empty body, which is a
decision, where a default would let the question go unasked. Keep your own
`gc_root_registration.rs` with an `EXPECTED_PROVIDERS` constant (probably
`0`), include the shared `test_support/gc_root_scan.rs` guard as the RP file
does, and assert at boot that `gc_roots::provider_count()` equals core's
`EXPECTED_PROVIDERS` plus yours — a real `assert!`, because device builds
compile `debug_assert!` out.

## 4. The filesystem

Two ways. The usual one: enable `picodroid-core`'s `littlefs` feature,
implement `FsBackingStore` (which extends `littlefs_rust::Storage`) over your
flash region, and register `set_hal_fs!(picodroid_core::fs::LittleFsHal)`.
The block arithmetic is `FsGeometry`'s — `resolve` for bounds,
`check_prog` for page alignment, `DEFAULT` for 4 KB / 256 B / 16 B, which
you can `assert!` your flash matches at compile time. Call
`picodroid_core::fs::init_device(store)` and then `spawn_worker()` before
the scheduler starts. Reserve the region in your linker script (the RP
family reads `__fs_start`/`__fs_end`).

The other way: implement `HalFs` yourself over whatever filesystem you
have, and never link a byte of LittleFS.

## 5. The debug bridge and installer (optional)

The protocol, the install orchestration, the sysmon encoder and the input
injection all live in `picodroid_core::pdb` and `picodroid_core::install`,
tested against mocks. You supply four things and hand them to
`run_pdb_task(transport, coordinator, sysmon, flash)` from your bridge task:

- **`PdbTransport`** — a byte pipe: `init`, `read_byte`, `read_byte_timeout`,
  `write_bytes`, `drain_tx`. If your tick source stops during a flash
  write (the RP2350's does), `read_byte_timeout` must busy-wait on a
  hardware timer instead of a tick-based wait.
- **`SysmonSource`** — fill a `SysmonSample` (up to `MAX_TASKS`
  `TaskSample`s) from your kernel's statistics.
- **`CoreCoordinator`** — stop the JVM and park the core that executes from
  the flash being written: `request_stop_and_park`, `wait_for_park`,
  `release`, `cancel_park_request`.
- **`PapkSlotFlash`** — three constants (where the boot-meta sector sits,
  the largest image, the sector size), `erase_range`, `program_range` and
  `reset`. `PapkSlot<YourFlash>` turns that into the `PapkFlash` the
  installer wants; you never write the erase rounding or page arithmetic.
  `InstallTransport` is satisfied by the bridge's own framing.

Read the installed app at boot with `install::read_mapped(slot_base, max)`
if your flash is memory-mapped. Wire layouts (frames, sysmon, input
events, keycode names) are types in the `pdb-protocol` crate, and the USB
vendor and product ID and strings are `pdb_protocol::usb`; core's
`pdb::usb_cdc` holds a reference CDC-ACM descriptor set built from them.
Never retype any of it.

## 6. The simulator

Two macro calls and no simulator code:

```rust
// glue.rs
#[cfg(any(test, feature = "sim"))]
picodroid_core::register_sim_platform! {
    gc_roots    = crate::gc_root_registration::register_all,
    boot_budget = crate::boot_budget::MODEL,   // static BootBudgetModel
    run_app     = crate::app::run_jvm,
}

// main.rs
#[cfg(any(test, feature = "sim"))]
picodroid_core::declare_sim_global_allocator!();

#[cfg(feature = "sim")]
fn main() {
    glue::sim_main()
}
```

`BootBudgetModel` lists the tasks your device creates at boot
(`BootTask { name, stack_bytes, sim_real }`), a TCB estimate, a queue
bucket, and your `default_stack_bytes` function. The simulator charges its
heap arena from it so the memory picture matches the device, and asserts at
the end of boot that the charges reconcile. `sim_real` is true for tasks the
simulator creates for real (they charge themselves), false for ones it only
models (a debug bridge, a WiFi driver, the kernel's own idle tasks).

Your simulator Cargo feature must be named `sim`; the generated `sim_main`
is gated on it. Your crate's own `cargo test` runs on the host and routes to
the shared simulator, but a dependency is never compiled with the
dependent's `cfg(test)`, so add:

```toml
[dev-dependencies]
picodroid-core = { path = "…", features = ["sim"] }
```

## 7. Boot, data and discipline

### `main.rs`

The entry point, panic handler, exception handlers, the device
`#[global_allocator]`, and your kernel's application hooks
(`vApplicationMallocFailedHook` should be a no-op so allocation failure
reaches the GC). Read the installed app, mount the filesystem, then start
your tasks.

### The JVM supervisor loop

The task that runs the app on a device loops for the life of the device,
because an install replaces the app. It stays in your crate — the "park for a
flash write" half encodes your flash topology — but it owes a fixed
checklist. `platforms/rp/src/boot_tasks.rs` is the reference:

1. Store the task's handle so the bridge and child tasks can wake it.
2. Loop: clear the stop flag; `run_app`; abort any child task delays;
   `picodroid_core::threads::wake_all_parked()`; wait until the count of
   live child tasks reaches zero.
3. Block until the bridge asks for a flash park (an install always opens
   with one), acknowledge, and block until it releases you or resets the
   chip; when released, go round again.
4. Never return from the task — its stack is the app's.
5. Do not put a stop check in your `HalClock::sleep`; shared code owns that.
6. Every one of those waits must re-check its condition after waking. The
   task collects notifications it did not ask for — `fs::with_fs` alone
   leaves one latched per call when the worker outranks the JVM — and a
   bare wait returns at once (the `bootcount` re-run of 2026-09).

### `build.rs`

Copy `platforms/rp/build.rs` and change the family-specific middle. It
`#[path]`-includes the shared `build_support/{config,board_cfg,boards,
freertos,network,papk,jvm_defaults}.rs` and must call, in order:
`board_cfg::resolve`, `boards::emit_board_imports`, your memory-layout and
kernel build, `board_cfg::emit_neutral(out, &board, Pins::Owned)`,
`config::emit_display_config`, `config::emit_touch_config`,
`board_cfg::emit_jvm_env_vars`, and the four `papk::*` embed calls.
`config::repo_root` and `config::is_embedded` are shared; do not derive
them by hand. The capability `cfg`s — `has_display`, `has_touch`,
`has_buttons`, `has_network`, `network_<type>`, `network_link_<kind>`, `any_sensor`,
`sensor_<kind>` — are emitted from `board.toml` by `board_cfg`, not set by
Cargo features. **Do not compile LVGL**: `picodroid-core`'s build script
owns it, and two builders mean duplicate symbols.

### Logging

`pd_trace!`, `pd_debug!`, `pd_info!`, `pd_warn!` and `pd_error!` from
`picodroid_core::pd_log`. On a device they are `defmt`; on the host,
`eprintln`. A family has no choice here: link a defmt sink (`defmt-rtt`)
and `panic-probe`.

### Guards you inherit

- The shadow-twin check in `scripts/pre-commit` (no same-path file in both
  trees, four allowlisted seam pairs).
- The cfg-hygiene check: never write `not(feature = "family-<yours>")` to
  mean "the simulator" — simulator builds keep the family feature on.
- `EXPECTED_PROVIDERS` and the GC-root scan.
- The core seam guard (`rtos::seam_guard`): shared code names no kernel
  primitive directly, so your `Rtos` is the only way it reaches one.
- If you are multi-core, write your own placement discipline; the RP
  family's `task_affinity.rs` and its source scan are the reference.

## The network

Only needed when a board sets `has_network = true`. The socket contract is
the `HalNet` trait (14 functions, registered with `set_hal_net!`). If your
family runs FreeRTOS and FreeRTOS+TCP, you do not implement it: core does,
and you write only the link driver for your chip. Design:
`docs/designs/network-seam-2026-09.md`.

**What core gives you** (feature `freertos-tcp`):

- `FreeRtosTcpNet` — the `HalNet` implementation over FreeRTOS+TCP sockets.
  Register it: `picodroid_core::set_hal_net!(picodroid_core::hal::freertos_tcp::FreeRtosTcpNet);`
- `run_link_task` — the bring-up every link needs, in order: driver init,
  MAC, IP stack start, bring-up, then the service loop.
- `picodroid-core/net-freertos-tcp/` — the shared C: `net_init.c` (stack
  start and the five FreeRTOS+TCP application hooks), `libc_str.c`, and the
  shared `FreeRTOSIPConfig.h` policy. Your `build.rs` compiles it, because
  it must see your `FreeRTOSConfig.h`.
- `picodroid_net_ip_event` — logs the up/down transitions.

**What you write:**

- `NetworkInterface_<X>.c`, against FreeRTOS+TCP's own `NetworkInterface_t`
  (`pfInitialise`, `pfOutput`, `pfGetPhyLinkStatus`, an RX path that posts
  `eNetworkRxEvent`). It must define
  `NetworkInterface_t *pxPicodroidNetLink_FillInterfaceDescriptor(BaseType_t, NetworkInterface_t *)`,
  the one name the shared glue binds to. A vendored driver with its own
  `pxXXX_FillInterfaceDescriptor` gets a five-line forwarder.
- A type implementing `NetLink` for the same chip: `KIND` (`LinkKind::Wifi`
  or `Ethernet`, for logs), `NAME`, `SERVICE_TIMEOUT_MS` (`None` when the
  vendored driver runs its own task), `init`, `mac`, `bring_up`, `service`.
- `src/hal/<family>/port/FreeRTOSIPConfig_family.h`: your IP-task affinity
  (required on a multi-core family), optionally priority and stack size.
- `uint32_t picodroid_port_entropy32(void)`, in Rust with `#[no_mangle]`:
  one random word per call, never failing (hardware RNG when you have one,
  a timer-mixed fallback when you do not).
- The spawn, in your boot code, on a task with your own core and stack:
  `run_link_task(MyLink)`.
- In `build.rs`: `build_support::network::build_freertos_tcp(&NetStackBuild { kernel_port_include, family_port_dir, link_sources: [your NetworkInterface_<X>.c, …], … })`.
  For an on-chip MAC add the vendored `portable/NetworkInterface/<Vendor>/NetworkInterface.c`,
  `Common/phyHandling.c` and `portable/NetworkInterface/include` there.
- In `board.toml`: `has_network = true` and `network_type = "<x>"`, with a
  row `("<x>", "wifi" | "ethernet")` in `build_support::board_cfg::KNOWN_NETWORK_TYPES`
  and a forward of `picodroid-core/network-<kind>` in your board feature.

Sockets are handles core never looks inside; addresses are IPv4 packed into
a `u32` (first octet in the top byte). Java learns the link kind from the
`network_link_<kind>` cfg: `PackageManager.FEATURE_WIFI` / `FEATURE_ETHERNET`
and `NetworkInfo.getType()`.

**Reference:** the RP family's cyw43 WiFi driver — `hal/rp/cyw43/`
(bindings and `Cyw43Link`), `hal/rp/port/net/` (`NetworkInterface_CYW43.c`,
`cyw43_port.c`), `hal/rp/entropy.rs`, and the spawn in `boot_tasks.rs`.

## HAL dispatch

Your family gets its own crate, so this is a `#[cfg]` inside *your*
`src/hal/mod.rs`, choosing between your peripherals and the shared
simulator:

```rust
// platforms/nrf/src/hal/mod.rs
#[cfg(any(feature = "sim", test))]
use picodroid_core::hal::sim as chip;

#[cfg(not(any(feature = "sim", test)))]
#[path = "nrf52/mod.rs"]
mod chip;

pub use chip::{adc, display, gpio, i2c, pwm, spi, system_clock, touch, uart};
```

Two things have no trait because they have no shared counterpart — a reset
vector and an XIP flash region are not things a host process has. The RP
family keeps `boot::clock_init` and its flash constants shape-asserted in
`src/hal/contract.rs`; do the same for whatever your boot needs.

## FreeRTOSConfig.h

Each MCU family provides its own `FreeRTOSConfig.h`. Key settings that differ
per family:

| Setting | Dual-core (RP) | Single-core (nRF52, STM32) |
|---------|---------------|----------------------------|
| `configCPU_CLOCK_HZ` | 125/150 MHz | varies |
| `configNUMBER_OF_CORES` | 2 | 1 |
| `configUSE_CORE_AFFINITY` | 1 | 0 |
| `configTICK_CORE` | 0 or 1 | N/A |
| `configSMP_SPINLOCK_*` | 26, 27 | N/A |
| `configSUPPORT_PICO_SYNC_INTEROP` | 1 | 0 |
| `configENABLE_FPU` | chip-dependent | chip-dependent |
| `configTOTAL_HEAP_SIZE` | 128 KB | depends on RAM |

Leave `configIDLE_AFFINITY` and `configTASK_DEFAULT_CORE_AFFINITY` unset on
a dual-core part: pinning the idle tasks pins the reaper, and a
`Thread.start`/`join` loop then leaks every finished child's stack. Settings
that are the same across families are in `platforms/rp/mcus/rp/FreeRTOSConfig.h`.

## Cargo features

Add your chip and family features to `Cargo.toml`:

```toml
[features]
chip-nrf52840 = ["dep:nrf52840-hal", "family-nrf"]
family-nrf = ["picodroid-core/family-nrf"]
board-my-board = ["chip-nrf52840", "picodroid-core/board-my-board"]
```

Each `board-*` feature forwards a marker feature of the same name to
`picodroid-core`, plus every capability feature the board declares
(`picodroid-core/sensor-bme688`, `picodroid-core/network-wifi`, …);
`board_cfg::assert_forwarded_features_match` fails the build otherwise. Add
the HAL crate as an optional, target-gated dependency:

```toml
[target.'cfg(target_arch = "arm")'.dependencies]
nrf52840-hal = { version = "...", optional = true }
```

## Build system

1. **Memory layout**: emit a `memory.x` from `build.rs` and select it based
   on the active MCU. The RP family generates its layout at build time via
   `boards::place_memory_x` rather than committing a file.
2. **FreeRTOS port**: `build_support/freertos.rs` compiles the kernel from
   the keys in your `mcus/<family>/<chip>.toml` (`freertos_port`,
   `pico_shim`, `freertos_port_extra_includes`, `freertos_c_defines`,
   `freertos_vector_aliases`, `init_array_segment`); populate the keys, no
   code changes.
3. **C shims**: the RP family needs `pico_shim_*.c` files that fake the
   pico-sdk C API the RP FreeRTOS SMP ports expect. Standard Cortex-M ports
   (ARM_CM4F, ARM_CM33) use CMSIS directly and need none.
4. **LVGL**: compiled by `picodroid-core`'s build script. Do not add it.

## .cargo/config.toml

Add the target entry:

```toml
[target.thumbv7em-none-eabihf]
runner = "probe-rs run --chip nRF52840_xxAA --protocol swd"
linker = "flip-link"
rustflags = [
  "-C", "link-arg=--nmagic",
  "-C", "link-arg=-Tlink.x",
  "-C", "link-arg=-Tdefmt.x",
]
```

## Single-core vs dual-core considerations

picodroid's RP port pins both the debug bridge and the JVM to core 0 (the
bridge preempts by priority); core 1 hosts a dedicated `flashpark` parker
task, plus the `cyw43` WiFi task on WiFi boards. Every task goes through
`task_affinity::spawn`, which makes create-and-pin one scheduler-atomic
step, and a source scan enforces that nothing else creates a task. Before
each erase/program window, core 0 notifies the parker via a cross-core task
notification (`hal/rp/core1_park.rs`), spins until core 1 reports parked,
and releases it when the window closes.

On single-core MCUs:

- **Task scheduling**: both tasks run on the same core; the bridge preempts
  the JVM via higher priority.
- **Flash writes**: erase/program disables interrupts, performs the
  operation, and re-enables — the same window the RP family uses.
- **No core parking**: there is no second core to park; omit the handshake.
- **No core affinity**: omit it from your `Rtos::spawn`.

## board.toml reference

Every physical board ships a `board.toml` under `boards/<name>/`. The build script parses it and emits Rust `cfg`s and `const`s — do not edit `boards/*/mod.rs` to configure a display or sensor, configure it here. All coordinates are GPIO numbers.

:::tip[App developers: what board.toml means for you]
You don't edit `board.toml` to write an app, but it determines what your app can do on a given board. A few keys are worth knowing: `lv_mem_kb` sets the LVGL render pool (smaller pools cap how many focusable list rows fit — see [Limits & memory budgets](/reference/limits/)); the presence of a `[touch]` section vs. `[[button]]` entries decides whether the board is touch- or button-driven (see [Button-only navigation](/guides/button-navigation/)); `idle_timeout_ms` controls display sleep; and `[jvm]` tunes the heap/GC tradeoff ([JVM tunables](/reference/jvm-tunables/)).
:::

### Top-level properties

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `mcu` | string | yes | `"rp2040"` or `"rp2350"` (the schema is family-agnostic). |
| `has_network` | bool | no | If `true`, compiles in the networking stack (FreeRTOS+TCP + a link driver). Needs `network_type`. |
| `network_type` | string | no | Required when `has_network = true`. Must be a row of `build_support::board_cfg::KNOWN_NETWORK_TYPES` (`"cyw43"` = wifi today); the build emits `network_<type>` and `network_link_<kind>` and checks the kind against the forwarded `picodroid-core/network-<kind>` feature. |
| `lv_dpi` | int | no | Override LVGL's reported DPI (default 130). Used for small-screen boards. |
| `lv_mem_kb` | int | no | LVGL render-pool size in KiB (default 64). |
| `idle_timeout_ms` | int | no | Idle time before the display sleeps (default 60000; `0` disables sleep). Only takes effect on boards with `[[button]]` entries. |
| `handle_slots` | int | no | Size of the LVGL object handle table (default 256). Must be a power of two between 32 and 4096. |
| `has_json` | bool | no | If `true`, ships `picodroid.json` (`JSONObject`/`JSONArray`/`JSONException` and the native node pool behind them). Off by default: a board that leaves it off drops those classes from its embedded SDK and compiles the parser out, and apps built for it fail the API contract if they reference them. |
| `framework_class_excludes` | list | no | Framework classes to leave out of this board's embedded SDK, to save flash. Classes owned by a feature switch (`has_json`) are added automatically; listing one by hand while the switch is on fails the build. |
| `linker_script` | string | no | Path to a custom `memory.x` (defaults to `mcus/<family>/<mcu>.x`). |

### `[display]` — display controller (ST7789 over SPI)

| Key | Type | Description |
|-----|------|-------------|
| `driver` | string | Documentation-only; the HAL hardcodes ST7789. |
| `spi_id` | int | SPI peripheral ID (0 or 1). |
| `spi_freq` | int | SPI clock in Hz (e.g. `62500000`). |
| `spi_sck`, `spi_mosi`, `spi_miso` | int | Optional SPI pad overrides; default to the chip's SPI pins (e.g. SPI0 SCK=GP2/MOSI=GP3 on RP2350). The Enviro+ Pack uses these to route SPI0 to GP18/GP19. |
| `pin_dc`, `pin_cs`, `pin_bl` | int | Data/command, chip-select, backlight GPIOs. |
| `pin_rst` | int | Reset pin (optional; some displays don't expose one). |
| `width`, `height` | int | Panel dimensions in pixels (**required** when `[display]` is present). |
| `madctl` | int (hex) | ST7789 memory-access-control register (controls rotation / mirroring). |
| `band_height` | int | LVGL partial-render band in pixels (**required**). |
| `scroll_limit` | int | LVGL scroll hysteresis threshold (**required**). |

Omit the whole `[display]` section for a headless board; the build then falls back to safe 320×240 defaults and leaves `has_display` unset.

### `[touch]` — touch controller (XPT2046 over SPI)

| Key | Type | Description |
|-----|------|-------------|
| `driver` | string | Currently only `"xpt2046"`. |
| `spi_freq` | int | SPI clock in Hz. |
| `pin_cs`, `pin_irq`, `pin_miso` | int | Chip-select, pen-down IRQ, MISO GPIOs. |
| `cal_x_min`, `cal_x_max`, `cal_y_min`, `cal_y_max` | int | Raw ADC bounds from touch calibration. |
| `swap_xy` | bool | Transpose X/Y axes (for rotated panels). |

### `[[sensor]]` — array of environmental sensors

| Key | Type | Description |
|-----|------|-------------|
| `kind` | string | Driver selector: `"bme688"` or `"ltr559"`. |
| `bus` | string | `"I2C0"` or `"I2C1"`. |
| `addr` | int | 7-bit I2C address (decimal or hex). |

Each entry here becomes a `Sensor` visible to [`SensorManager`](/api/sensors/).

### `[[button]]` — array of hardware buttons

| Key | Type | Description |
|-----|------|-------------|
| `pin` | int | GPIO number. |
| `lv_key` | string | One of `"PREV"`, `"NEXT"`, `"ENTER"`, `"ESC"` — drives LVGL focus navigation. |
| `keycode` | int | Android `KeyEvent.KEYCODE_*` value delivered to Java listeners. |

Declaring at least one `[[button]]` enables the idle display-sleep + wake-on-button feature (the sleep delay is `idle_timeout_ms`, default 60 s; set it to `0` to keep the panel always on, as `pico_enviro_mon` does). See [api/ui.md → Key events](/api/ui/#key-events) and the [Button-only navigation](/guides/button-navigation/) guide.

### `[background_pool]` — optional thread-pool tuning

All keys optional; defaults in parentheses.

| Key | Type | Description |
|-----|------|-------------|
| `threads` | int | Worker count (4), range 1..=32. |
| `priority` | int | Worker priority (15). Must be 15 — the JVM's own tier — and the build fails otherwise: the pool runs Java, and Java runs on one tier by design. |
| `stack_bytes` | int | Per-worker stack in bytes (4096). |
| `queue_depth` | int | Shared job queue depth (32). |

Surfaced via [`Executors.backgroundExecutor()`](/api/system/#picodroidconcurrentexecutors).

### `[jvm]` — optional CPU↔memory tradeoff knobs

Compile-time `pub const`s sourced from this section, all optional. See [JVM tunables](/reference/jvm-tunables/) for the full schema, tuning workflow, and worked recipes.

| Key | Type | Description |
|-----|------|-------------|
| `gc_alloc_threshold` | int | Allocations between auto-GC cycles (256), range 16..=8192. |
| `slot_chunk_shift` | int | Chunk size = `1 << shift` for heap slot storage (6), range 3..=8. |
| `inline_array_data` | int | Array elements held inline rather than in the arena (8), range 0..=32. |
| `activity_stack_depth` | int | Max nested Activities (8), range 1..=32. |
| `pending_op_queue` | int | Max queued startActivity/startService ops per frame (8), range 1..=64. |
| `prereserve_*` | int | Six pre-reservation sizes for the heap's slot chunks and arenas; see the JVM tunables page. |

## Verification

After implementing your port, run the full pre-commit suite:

```bash
# Sim smoke test (verifies picodroid business logic is not broken)
./scripts/sim.sh --app helloworld
perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky

# Full suite: formatting, clippy (all targets), build, tests
./scripts/pre-commit
```

`cargo test -p picodroid-core` includes the checklist guard: if you add a
seam trait or registration macro to core, it fails until this page and
`porting.rs` name it.

For on-device testing, flash with your chip's target:

```bash
cargo run --target thumbv7em-none-eabihf \
          --no-default-features --features chip-nrf52840
```

## Reference implementation

The RP family (`platforms/rp/`) is the reference implementation. Study these
files for patterns and conventions:

| File | What it demonstrates |
|------|---------------------|
| `src/glue.rs` | Every registration in one place; delegating trait impls |
| `src/boot_tasks.rs` | Task topology and the JVM supervisor loop |
| `src/task_affinity.rs` | Dual-core placement and the scan that enforces it |
| `src/pdb/platform.rs`, `src/pdb/coordinator.rs` | `PdbTransport`, `SysmonSource`, `CoreCoordinator` |
| `src/packagemanager/mod.rs` | `PapkSlotFlash` over two flash primitives |
| `src/fs/storage.rs` | `FsBackingStore` over a linker-carved flash region |
| `src/boot_budget.rs` | The boot-budget model the simulator charges |
| `src/hal/rp/gpio.rs` | Direct register access, the interrupt, `GpioEventRing` in use |
| `src/hal/rp/flash.rs` | XIP-disabled flash operations from RAM, with core 1 parked (`core1_park.rs`) |
| `src/hal/rp/pdb_usb/mod.rs` | USB CDC ISR → queue pattern behind `PdbTransport` |
