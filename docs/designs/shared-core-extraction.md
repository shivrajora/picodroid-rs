# Design: Extract the platform-agnostic core into `picodroid-core`

> Produced 2026-07-26 by the code-structure audit session (three parallel
> exploration agents mapping structure / history / coupling, then a design
> pass). Amendments are appended at the bottom and OVERRIDE the design body
> where they conflict. Execute from this doc; update it if reality diverges.

## 0. Why this exists

Two reasons the framework runtime sits inside the RP binary crate today —
neither of them a design decision about layering:

1. **Incidental.** Code began at the repo root (`301f577`, 2026-03-05).
   `f746e93` (2026-05-04, "Introduce platforms/ directory") was a mechanical
   229-file rename, almost all `R100` (byte-identical). `git log --follow`
   on `platforms/rp/src/main.rs` walks straight back to the initial commit:
   the file was never *moved into* `platforms/rp`, the directory was wrapped
   around it.
2. **Substantive.** The staged extraction of 2026-05-02 (`ad46157` skeleton →
   `b107f81` zero-change modules → `1b3602b` task_priority → `9139f18`
   esp workspace → `5f2dbb8`) deliberately stopped at the `crate::hal::`
   boundary. `1b3602b`'s message is the whole rationale:

   > NOTE: `app.rs` / `lifecycle.rs` / `service_lifecycle.rs` /
   > `system/picodroid/` / `fs/` are **NOT** moved here. All of these call
   > `crate::hal::` directly (display, flash, uart, etc.) and **can't move to
   > picodroid-core until a HAL abstraction layer is defined** so chip
   > implementations stay in binary crates while shared logic lives in the
   > lib. That refactor is a separate milestone; it does not block the ESP
   > workspace split (Stage 4) because `picodroid-esp` carries its own
   > simplified app bootstrap.

   That milestone was never scheduled. ESP — the only force that would have
   driven it — sidestepped it with a stripped-down bootstrap plus a
   copy-pasted HAL shape (a duplicated `contract.rs` and 17 sim-stub twins
   that drifted), then was removed entirely on 2026-07-25 (`8300bf8`).

The 2026-07 audit (`docs/code-health-audit-2026-07.md` §5, §7) measured the
result and deferred it: *"With ESP removed, extracting this layer loses its
near-term driver — record it as the known cost of a future second family, not
a current work item."* This design pays that cost deliberately, ahead of a
second family, while the measurements are fresh.

## 1. Current state (verified in source, 2026-07-26 at `74cf8c9`)

`platforms/rp/src` is ~35.6K lines. Category rollup:

| Category | LOC | What |
|---|---:|---|
| **CORE** (movable) | ~20,600 | `system/native_handler/` (3.3K), `system/picodroid/` (graphics 11.6K, net 1.2K, pio 1.5K, sensors 1.4K, os/util), `lifecycle.rs` (1.9K), `service_lifecycle.rs` (0.7K), `system/executors/` (0.8K), `monitor_store`, `mem_diag`, `notification`, most of `app.rs` |
| **RP-specific** | ~4,900 | `hal/rp/**` (Rust + Pico SDK C shims), `fs/storage.rs`, `fs/worker.rs`, `pdb/**`, `packagemanager/**`, `boards/**` |
| **SIM-specific** | ~2,960 | `hal/sim/**` (minifb display/touch), `sim_allocator.rs`, `sim_heap4.rs`, `fs/storage_host.rs` |
| **MIX** | ~5,200 | `main.rs`, `boot_budget.rs`, and the CORE files above that also touch freertos/OUT_DIR |

The good news, and the reason this is tractable: **`system/` contains zero
RP-silicon coupling.** `rg 'embassy_rp|rp_pac|rp2040|rp2350|rp235x|rp_pico|cortex_m|critical_section|panic_probe|defmt_rtt'`
over `platforms/rp/src/system/` returns nothing. All embedded hits are
`freertos_rust`. That isolation is the product of prior deliberate work
(`ARCHITECTURE.md:97-103`) and it means the blockers are **mechanical, not
architectural**:

### 1.1 Blocker A — `include!(OUT_DIR/…)` in core files

OUT_DIR is per-crate; a file that includes from it cannot move to another
crate unless that crate's build script generates the same artifact. The
movable files that do this:

| File | Artifact |
|---|---|
| `system/picodroid/graphics/lvgl/events.rs` | `button_config.rs` |
| `system/picodroid/graphics/lvgl/handle_table.rs` | `handle_table_config.rs` (new, from P1-9) |
| `system/native_handler/state.rs` | `jvm_state_config.rs` |
| `system/executors/background_pool.rs` | `background_pool_config.rs` |
| `system/picodroid/hardware/sensors/sampler.rs` | `sensor_table.rs` |
| `system/mem_diag.rs` | `heap_config.rs` |
| `lifecycle.rs` | `sleep_config.rs` |
| `app.rs` | `apk_data.rs`, `framework_mapping_version.rs`, `jvm_prereserve_config.rs` |

(Staying platform-side and therefore not a problem: `hal/rp/*` and
`hal/sim/*` display/touch configs, `boards/mod.rs`, `packagemanager/flash`.
Note `hal/sim/display.rs` also reads `display_config.rs` + `button_config.rs`
— relevant at Stage 8 when the sim HAL moves.)

### 1.2 Blocker B — direct `freertos_rust`

Six movable files (plus `app.rs`), against a dependency declared only under
`[target.'cfg(target_arch = "arm")'.dependencies]` of the binary crate:

| File | Uses |
|---|---|
| `system/executors/tick_source.rs` | `Timer`, `Duration` |
| `system/executors/main_queue.rs` | `Queue` |
| `system/executors/background_pool.rs` | `Queue`, `Task`, `TaskPriority` |
| `system/monitor_store.rs` | `MutexRecursive`, `MutexInnerImpl` |
| `system/native_handler/os.rs` | `Task` (spawn, `.core_affinity(0b01)`), `Task::current` |
| `system/picodroid/hardware/sensors/sampler.rs` | `Semaphore`, `Task` |
| `app.rs` | `CurrentTask::delay` |

Each already carries an inline `mod backing` / `mod device` + `mod sim` cfg
split — that shape is what the RTOS trait formalises.

### 1.3 Blocker C — `hal` is a module of free functions

~90 `crate::hal::` call sites in `system/`. HAL CONTRACT v1
(`platforms/rp/src/hal/mod.rs:9-118`) is already a complete, written
specification of the required symbols, and `hal/contract.rs` type-asserts it
per binary — but a *module* cannot cross a crate boundary the way a trait
can.

### 1.4 Other verified coupling points

- **`BAND_BUF`** (`lvgl/lifecycle.rs:19-28`) is a `static` sized from
  `hal::display::{WIDTH, BAND_HEIGHT}` consts. A shared-crate static cannot
  be sized by a downstream crate's constants.
- **`app.rs`** holds the baked-in PAPK blob (a link-time artifact),
  `SHARED_HEAP`, and the loader/bootstrap functions, entangled.
- **GC roots fan out** from `native_handler/mod.rs:193-253` across
  `graphics::display`, `hardware::sensors`, `service_lifecycle`, `lifecycle`,
  and ~13 widget/listener-map providers — i.e. across nearly the whole
  movable body. Splitting that set across a crate boundary fails *silently*
  (premature collection), not at compile time. This project has been burned
  by exactly that class of bug repeatedly.
- **`#[global_allocator]`** (`main.rs`, `sim_allocator.rs`) is only legal in
  the binary crate.
- **LVGL C is compiled by `platforms/rp/build.rs`** even though the FFI
  declarations live in `picodroid-core`. Benign today; load-bearing the
  moment core contains code that calls `lv_*`.
- **`boot_budget.rs`** is genuinely platform policy (chip-gated stack sizes)
  but `native_handler/os.rs` reads `JVM_THREAD_STACK_WORDS` from it.

## 2. Decisions

Confirmed with the maintainer before this doc was written:

1. **Boundary mechanism: traits + link-time registration macros** — the
   `critical-section` / `defmt` pattern. Rejected alternatives: boot-time
   `dyn` registration (adds an init-ordering hazard on paths that run before
   `main` completes setup, plus vtable indirection on the band-flush hot
   path), and generic type parameters (the framework is full of `static`s,
   which cannot take type parameters — it would be viral and force a
   rewrite of state ownership).
2. **Target: expand `picodroid-core`** — no new crate. It is already the
   audit-designated *"landing zone if a second family returns"*, its build
   script already re-emits capability cfgs from forwarded features, and the
   framework's heaviest dependencies (`lvgl_ffi`, `dispatch_sites`,
   `framework_classes`) already live there.
3. **Sequencing: P1-6 and P1-9 land first** (both did: `009503c`/`74cf8c9`
   and `a1063ed`/`3d441fb`/`9ef80ae`). The extraction then relocates
   hardened, test-pinned code rather than racing it.

## 3. The seam

All new code lands in `picodroid-core`. Every crossing is static dispatch
through `#[no_mangle] extern "Rust"` shims. **No `dyn` on the seam** — the
RP2040 has ~40 KB of flash headroom and LTO is not available (it makes the
image bigger; `scripts/lib.sh:249` strips it for embedded links).

### 3.A HAL traits — `picodroid-core/src/hal/`

One trait per HAL CONTRACT v1 module, signatures lifted verbatim from
`hal/contract.rs`:

| Contract module | Trait | Notes |
|---|---|---|
| `display` | `HalDisplay` | 8 fns. The `WIDTH`/`HEIGHT`/`BAND_HEIGHT`/`SCROLL_LIMIT` consts leave the contract and become generated board config (§3.D) — consts cannot be trait-dispatched at the sizes we need them (§3.E). |
| `gpio` | `HalGpio` | 11 fns. `Pull`, `EdgeTrigger`, `GpioEvent` move into `picodroid-core::hal::types` (verify at implementation that the rp and sim `GpioEvent` structs are field-identical — `key_debounce` reads `t_us`). |
| `system_clock` | `HalClock` | `sleep`, `elapsed_realtime_nanos`. |
| `touch` | `HalTouch` | 7 fns incl. the three PDB injection overrides. |
| `i2c` | `HalI2c` | 4 fns. |
| `adc`, `pwm`, `spi`, `uart` | `HalAdc`, `HalPwm`, `HalSpi`, `HalUart` | pio passthroughs, 2-4 fns each. |
| `net` (cfg `has_network`) | `HalNet` | 15 fns. `NetError` becomes core-owned; sockets stay `*mut c_void`. |
| *(new)* | `HalFs` | Exactly the `native_handler/io.rs` backend surface: `exists`/`is_file`/`is_dir`/`length`/`delete`/`mkdir`/`rename`/`truncate`/`read_at`/`write_at`/`list`. Paths and bytes only — core stops knowing about `littlefs_rust` types; mount/worker/flash stay in `platforms/rp/src/fs/`. |
| `boot`, `flash`, `pdb_usb`, `delay`/`input_pin`/`output_pin`/`spi_bus` | **not traits** | Consumed only by bin-side code (`main.rs`, `pdb/`, `packagemanager/`, family-internal driver wiring). `hal/contract.rs` keeps asserting these. |

**Facade.** `picodroid-core/src/hal/facade/` re-creates today's call
spellings as free functions:

```rust
// picodroid-core/src/hal/facade/display.rs
extern "Rust" {
    fn __pd_hal_display_update_window();
    fn __pd_hal_display_write_pixels(data: &[u8]);
    // …
}
pub fn update_window() { unsafe { __pd_hal_display_update_window() } }
pub fn write_pixels(data: &[u8]) { unsafe { __pd_hal_display_write_pixels(data) } }
```

Moved files therefore keep `hal::display::update_window()` textually
unchanged — `crate::hal` simply resolves to core's facade instead of the
bin's module. This is what keeps ~90 call sites out of the diff.

**Registration.** One macro per subsystem so a platform can adopt
incrementally, plus an umbrella:

```rust
picodroid_core::set_hal_display!(RpDisplay);
// expands to, for each trait fn:
//   #[no_mangle] extern "Rust" fn __pd_hal_display_update_window() {
//       <RpDisplay as ::picodroid_core::hal::HalDisplay>::update_window()
//   }
```

Because the macro generates both the `extern` declaration (core side) and
the shim (platform side) from one definition, **signature drift is a compile
error at the `set_hal_*!` site** — the macro subsumes what `contract.rs` did
for trait-covered modules. Cost: ~58 HAL + ~15 RTOS + ~8 hook wrappers ≈ 80
one-line functions; one extra direct call per crossing, which is what a
cross-module call already costs in a no-LTO build. Measure the flash delta
at Stage 2 against the rp2040 release gate; budget ≤ ~2 KB.

### 3.B RTOS trait — `picodroid-core/src/rtos/`

Covers exactly what §1.2 uses, nothing speculative:

```rust
pub enum TaskKind { JvmChild, BgWorker, Sensor }
pub struct TaskSpec {
    pub name: &'static str,
    pub kind: TaskKind,
    pub priority: u8,             // FreeRTOS tier from task_priority (already in core)
    pub stack_bytes: Option<u32>, // None = platform default for this kind
}
pub enum Timeout { None, Ms(u32), Forever }

pub unsafe trait Rtos {
    fn spawn(spec: &TaskSpec, entry: fn(u32), arg: u32) -> bool;
    fn queue_create(depth: usize) -> RawQueue;
    fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool;
    fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32>;
    fn mutex_recursive_create() -> Option<RawMutex>;
    fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool;
    fn mutex_recursive_unlock(m: RawMutex);
    fn sem_binary_create() -> RawSem;
    fn sem_give(s: RawSem);
    fn sem_take(s: RawSem, t: Timeout) -> bool;
    fn tick_timer_start(period_ms: u32, cb: fn());
    fn tick_timer_pause();
    fn tick_timer_resume();
    fn delay_ms(ms: u32);
}
```

`stack_bytes` is **bytes, never words**. FreeRTOS takes words, ESP-IDF takes
bytes (`ARCHITECTURE.md:99-103`); baking the unit into the seam is precisely
the kind of family assumption this refactor exists to remove.

Platform policy stays platform-side, inside the impl:

- The RP `spawn(JvmChild)` applies `.core_affinity(0b01)`, sources its stack
  from `boot_budget::JVM_THREAD_STACK_WORDS`, and does the
  `pdb::pending::register_child_task` / `deregister_child_task` bracketing —
  i.e. today's `native_handler/os.rs:56-96` body becomes the impl body.
- The sim `spawn(JvmChild)` keeps the `PICODROID_PARITY_STRICT` panic and the
  boot-budget heap charge.
- `queue_*` serves both `main_queue` (u32 words) and `background_pool` (u16
  widened) unchanged; the sim backing is today's `Mutex<VecDeque>` + `Condvar`
  relocated verbatim.

### 3.C Platform hooks — `picodroid-core/src/host/`

The residue that is neither HAL nor RTOS:

```rust
pub trait PlatformHooks {
    fn stop_requested() -> bool;                    // pdb::pending::is_stop_jvm
    fn shared_heap() -> &'static mut SharedJvmHeap; // SHARED_HEAP stays bin-side
    fn task_stack_bytes(kind: TaskKind) -> u32;     // boot_budget policy
    fn heap_bypass_enter();
    fn heap_bypass_exit();                          // sim_allocator::bypass
    fn heap_checkpoint(label: &'static str);
    fn native_heap_stats() -> NativeHeapStats;      // vPortGetHeapStats / heap4_stats
    fn sim_charge_thread_spawn();
}
```

`heap_bypass_*` matters more than it looks: the sim's heap cap must not
charge the host-only minifb window buffer (~900 KB), which would OOM any low
`-l` limit. That bypass is currently a direct `sim_allocator::bypass()` call
from `hal/sim/display.rs`; the hook is what lets the sim HAL move in Stage 8
while the allocator stays behind.

**Registration discipline** — the leaf artifact registers, `defmt`-style:

| Where | cfg | Registers |
|---|---|---|
| `platforms/rp` | `not(any(test, feature = "sim"))` | RP HAL + FreeRTOS RTOS + device hooks |
| `platforms/rp` | `any(test, feature = "sim")` | core's sim/std impls + `sim_allocator`-backed hooks |
| `picodroid-core` | its own `cfg(test)` | headless stubs, so `cargo test -p picodroid-core` links |

Note the cfg is `feature = "sim"`, **not** `not(feature = "family-rp")`: sim
builds keep `family-rp` active through the board feature chain
(`docs/parity-audit.md` BLD-02, guarded by a count check in
`scripts/pre-commit:40-53` that expects exactly 4 such gates repo-wide).
No collision is possible between the three rows — `cfg(test)` is
per-crate-under-test, and exactly one registration is ever in a given link.

### 3.D Config channel

`picodroid-core/build.rs` must learn the active board.

- **Features.** `picodroid-core` gains empty `board-testbench-rp2040`,
  `board-testbench-rp2350`, `board-testbench-rp2350w`,
  `board-pico-enviro-mon`, plus forwarded `handle-table-32`,
  `parity-metrics`, `parity-fbhash`, `mem-diag`. `platforms/rp`'s board
  features enable the matching core feature — the same forwarding already
  used for `sensor-*`, `network-cyw43`, `sim`, `family-rp`.
- **Board discovery.** `build_support/config.rs` gains
  `find_board_dir(manifest_dir, name)` searching `<manifest>/boards/<name>`
  then `<manifest>/../platforms/*/boards/<name>`. `resolve_active_board()`
  keeps scanning `CARGO_FEATURE_BOARD_*`. This preserves the invariant that
  `cargo build -p picodroid` with default features works with **zero env
  vars**: default feature → forwarded core feature → env var visible to
  core's build.rs → board.toml found one directory up. `rerun-if-changed`
  paths become absolute, since the file now lives outside core's manifest
  dir.
- **Neutral vs pin split.** board.toml's *format* does not change; what
  changes is which build script emits what.

  | Emitter | Artifacts |
  |---|---|
  | `picodroid-core/build.rs` | `heap_config.rs`, `sensor_table.rs`, `sleep_config.rs`, `jvm_state_config.rs`, `jvm_prereserve_config.rs`, `background_pool_config.rs`, `handle_table_config.rs`, `button_config.rs`, display dims as `board_cfg::display`, and the `has_display`/`has_touch`/`has_buttons`/`has_network`/`network_*`/`sensor_*`/`any_sensor` cfgs |
  | `platforms/rp/build.rs` | `memory.x` placement, FreeRTOS/cyw43/FreeRTOS-TCP C builds, pin-bearing `display_config.rs` + `touch_config.rs` for `hal/rp/*`, `board_imports.rs`, `apk_data.rs`, `papk_flash_init`, the JVM env-var safety net |

  Boardless builds (`cargo build -p picodroid-core` alone) keep today's safe
  defaults — the `None` arms already exist in `emit_heap_config` and
  `emit_display_config`.
- **Button pins stay in core.** `init_button_pins` (`events.rs:337-344`)
  already speaks only contract GPIO (`set_input`/`enable_edge_irq`/
  `init_gpio_irq`), the pin is the join key between `GpioEvent.pin` and
  keycodes (`pin_to_keycode`), and the sim's keyboard emulation consumes the
  same table. Moving button init platform-side would duplicate the table on
  both sides and invent a boot-ordering contract for no silicon-independence
  gain. Pins remain opaque `u8`s to core. `keycode_to_pin` stays exported for
  PDB `CMD_INPUT`.
- **Drift guard.** The bin asserts the two generated views agree:

  ```rust
  const _: () = assert!(picodroid_core::board_cfg::display::SCREEN_WIDTH == hal::display::WIDTH);
  ```

  (and `HEIGHT`/`BAND_HEIGHT`/`SCROLL_LIMIT`), so one board.toml can never
  produce two disagreeing artifacts.

### 3.E `BAND_BUF`

Re-source it from core-generated `board_cfg::display::{SCREEN_WIDTH,
BAND_HEIGHT}`. It stays a compile-time-sized `static` with zero runtime cost
and no API change. Rejected: platform-allocated `&'static mut [u8]` handed in
at init (runtime indirection on the flush path, plus an init-order contract),
and a max-size const (wastes RAM on 264 KB parts). Same treatment for
`graphics/display.rs:62-80`, `fps_overlay.rs:95`, `calibration.rs`.
`pdb/input.rs` stays platform-side and keeps reading platform-side consts.

### 3.F Logging

Moved code's paired `#[cfg(not(feature = "sim"))] defmt::… /
#[cfg(feature = "sim")] println!` arms are wrong inside core: a host
**non-sim** test build would take the defmt arm and demand a global logger.
Core gains `pd_log::{trace, debug, info, warn, error}` keyed on
`cfg(any(target_arch = "arm", target_arch = "xtensa"))` → defmt, else
`eprintln!`. Each move stage converts its files mechanically; the conversion
is net-negative on LOC.

### 3.G GC roots — registry first

The root set spans ~17 static providers plus 4 module delegates
(`native_handler/mod.rs:193-253`) reaching across nearly the entire movable
body. Two options: move the whole cluster atomically (not commit-sized in
the P1-5 sense, and unreviewable), or introduce indirection first. We do the
latter — which is also audit **P2-17**, delivered here as an enabler:

```rust
// picodroid-core/src/gc_roots.rs
pub type RootProvider = fn(&mut dyn FnMut(pico_jvm::types::Value));
pub fn register(p: RootProvider);            // fixed [Option<RootProvider>; 32]
pub fn visit_all(v: &mut dyn FnMut(Value));  // no alloc, no CAS (thumbv6m)
pub fn provider_count() -> usize;
```

`gc_visit_roots` keeps only its `&self` own-state visits (activity stack,
retained intents, pending ops) and calls `visit_all()`. Registration happens
once from `register_all_roots()` at the top of `run_jvm_with`, before any
class is loaded and therefore before any GC can run. As each module moves,
its single `register(…)` line moves from the bin's list to core's list **in
the same commit as the file** — the provider is never absent from the union.

This also breaks the `native_handler` → `graphics` dependency cycle (§4.3 of
the audit), P2-17's second payoff.

Three guards, because this failure mode is silent:

1. A test asserting `provider_count()` equals an expected constant, bumped
   explicitly in every commit that moves a provider.
2. An `rg`-based test: every `fn visit_.*roots` definition under
   `platforms/*/src` and `picodroid-core/src` must be reachable from either
   the registry table or `gc_visit_roots`.
3. `./scripts/sim.sh --app gcstress` in every stage's verification from
   Stage 2 onward.

### 3.H LVGL build ownership

Today `picodroid-core/src/lvgl_ffi.rs` declares the `extern "C"` surface
while `platforms/rp/build.rs:120` compiles the C. The moment core contains
`lv_*` callers, `cargo test -p picodroid-core` fails to link.

At Stage 4a, in **one commit**: `picodroid-core/build.rs` starts calling the
already crate-neutral `build_support/lvgl.rs::build(out, &board_props,
repo_root)`, and `platforms/rp/build.rs` drops its call. `cc`-produced static
libs propagate transitively from a dependency's build script, so firmware
links core's copy; doing both sides together avoids duplicate `lv_*` symbols.
Board props (`lv_dpi`, `lv_mem_kb`) come from core's board resolution
(§3.D); boardless builds pass `None` and get `lv_conf.h` defaults. Compile
unconditionally on host (so workspace tests always link) and on arm when
`family-rp`. The `-fshort-enums` host-parity handling is already inside
`build_support/lvgl.rs:72-77` and comes along free.

## 4. Stages

Every stage ends green: `./scripts/pre-commit` (must print
`==> All checks passed.`) plus `./scripts/sim.sh --app helloworld`,
`--app benchmark`, and `perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky`,
plus the stage's own checks. From Stage 2 on, add `--app gcstress`.

| # | Stage | Scope |
|---|---|---|
| 0 | Design doc | This file. |
| 1 | Config channel | `find_board_dir`; core build.rs emits the neutral artifact set + capability cfgs; feature forwarding both sides. **No code moves.** ~5 files, ~350 LOC. |
| 2 | Seam scaffolding + GC registry | (2a) core `hal/{mod,types,traits,facade}`, `rtos`, `host`, `pd_log`, the three registration macros, core `cfg(test)` stubs; new `platforms/rp/src/glue.rs` with device + sim arms delegating to today's `hal::rp`/`hal::sim`/`freertos_rust`/std. (2b) `gc_roots.rs`; rewrite the fan-out; `register_all_roots()`; all three guards. (2c) record the rp2040 release flash delta here. ~14 new files, ~1.7K LOC. |
| 3 | RTOS consumers | `git mv` `system/executors/*` and `system/monitor_store.rs` → core (monitor_store carries 2 of the 4 `not(feature = "family-rp")` gates — move the text verbatim). Rewire `os.rs` and `sampler.rs` in place. Bin re-exports at the old paths. ~1.3K LOC. |
| 4a | LVGL flip + leaves | Build ownership flip (§3.H, same commit both sides) + `git mv` `lvgl/{listener_map,key_filter,key_debounce,edit_mode,drawable,handle_table,animations}.rs`. Delete their `#[path]` test shims from `main.rs` — tests run in-crate now. ~1.9K LOC. |
| 4b | LVGL engine | `git mv` `lvgl/{mod,events,lifecycle,calibration,fps_overlay,view_ops}.rs`. `button_config` from core OUT_DIR; `BAND_BUF` per §3.E; `is_stop_jvm` → `host::stop_requested()`. ~2.2K LOC. |
| 4c | Widgets | `git mv` `lvgl/widgets/` and `graphics/{widgets,view.rs,display.rs,assets.rs,gfx/}`. Mechanical import re-roots, `pd_log` conversion, `pub(in …)` path re-rooting. Split into two commits if review size demands. ~8.6K LOC. |
| 5 | sensors / pio / net / os / util | `git mv` `system/picodroid/{hardware,pio,net,os,util}`. Sampler's three-way split resolves: `sensor_table` from core OUT_DIR, spawn via `TaskSpec{kind: Sensor}`, `HalI2c` facade feeding core's existing generic drivers. ~4.6K LOC. |
| 6 | native_handler + diagnostics | `git mv` `system/native_handler/` + `system/{notification,mem_diag}.rs`. `io.rs` → `HalFs`; `mem_diag` → `host::native_heap_stats()`; the `every_native_class_is_registered` and P1-6 method-level cross-check tests move in-crate. ~4.25K LOC. |
| 7 | lifecycle + app split | `git mv` `lifecycle.rs`, `service_lifecycle.rs`. Split `app.rs`: loaders + `run_jvm_with` body → `picodroid-core/src/boot.rs::run_app(&[u8])`; bin keeps the blob, `SHARED_HEAP`, hooks impl, thin `run_jvm()`. Core gains a runtime dep on `papk-format`. ~2.9K LOC. |
| 8 | Sim HAL → core | `git mv` `hal/sim/*` (minus `boot`/`flash`/`pdb_usb` stubs) as `SimHal*` impls; `sim_allocator::bypass` → `host::heap_bypass`; `minifb` dep moves to core. Kills the porting guide's "copy `hal/sim/`" step — the mechanism that produced ESP's 17 drifting twins. ~1.7K LOC. |
| 9 | Guards + docs | Shadow-twin guard in pre-commit; retire trait-covered `contract.rs` asserts; reconcile the feature-vs-board.toml cfg dual source; refresh `ARCHITECTURE.md`, the porting guide, `picodroid-core/Cargo.toml`'s header, and the audit progress note. Final HW smoke + flash delta. |

**Execution discipline, every stage:**

- Every move is `git mv` **plus** a bin-side `pub use picodroid_core::X;`
  re-export at the old path. Never leave a same-named file behind: the
  2026-05 partial extraction left 4 dead shadowed twins, 2 of which
  *silently diverged* for three months before `fc896b3` cleaned them up.
- Never introduce a new `not(feature = "family-rp")` gate (pre-commit
  count-guards exactly 4).
- Modules that stay in `platforms/rp` and are hardware-free still need their
  `#[path]` test shim in `main.rs`; modules that move to core get ordinary
  in-crate tests and their shim deleted.

## 5. End state

**`platforms/rp` keeps (~12K LOC):** `main.rs` (entry, exception handlers,
FreeRTOS hooks, `#[global_allocator]`), `glue.rs` (the three registration
macro invocations, device + sim arms), `hal/rp/**` plus a slim `hal/mod.rs`
and the bin-side `boot`/`flash`/`pdb_usb` sim stubs, a thin `app.rs`,
`boot_budget.rs`, `crc32.rs`, `fs/**`, `pdb/**`, `packagemanager/**`,
`sim_allocator.rs` + `sim_heap4.rs`, `boards/**`, `mcus/**`, and a `build.rs`
reduced to C builds, `memory.x`, pin configs, and APK embedding.

**Dependency graph:** `picodroid` → `picodroid-core` → {`pico-jvm`,
`papk-format`, `compat`, `embedded-hal`, `defmt`, `minifb` (host-only)};
`picodroid` additionally → {`freertos-rust`, `cortex-m`(`-rt`),
`rp-pico`/`rp235x-hal`, `defmt-rtt`, `panic-probe`, `libc`}. Plus a link-time
back-edge: ~80 `__pd_*` symbols resolved by the bin's registrations. LVGL C
compiled by core's build script only.

**What a future `platforms/stm32` must provide** — the acceptance test for
this whole exercise:

1. A bin crate: `main.rs`, linker/memory setup, `#[global_allocator]`;
   `boards/<name>/board.toml` + `mcus/stm32/<mcu>.toml`; a `board-*` →
   `chip-*` → `family-stm32` feature chain forwarding to core.
2. Impls of `HalDisplay`/`HalGpio`/`HalClock`/`HalTouch`/`HalI2c`/`HalAdc`/
   `HalPwm`/`HalSpi`/`HalUart` (+ `HalNet`, `HalFs` if the board has them).
3. An `Rtos` impl (FreeRTOS port, Embassy adapter, whatever) with its
   `TaskKind` → stack/affinity policy, and a `PlatformHooks` impl.
4. One `glue.rs` invoking the three registration macros.
5. Optionally a PDB transport and a PAPK flash region for install support.
6. **Zero edits to `picodroid-core`.** The simulator, every widget, the
   lifecycle, the JVM natives, sensor plumbing, and the registry guards all
   come for free.

## AMENDMENTS

*(none yet — append here as execution diverges; amendments override the body
above)*
