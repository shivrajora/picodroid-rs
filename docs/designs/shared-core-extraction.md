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
4. One `glue.rs` invoking the registration macros (`set_hal!`, `set_hal_fs!`,
   `set_hal_net!`, `set_rtos!`, `set_platform_hooks!`, `register_sim_platform!`).
5. Optionally the four debug-bridge/install traits (`PdbTransport`,
   `SysmonSource`, `CoreCoordinator`, `PapkSlotFlash`) and a PAPK flash slot.
   *(Updated 2026-09-03; `picodroid_core::porting` is the live list.)*
6. **Zero edits to `picodroid-core`.** The simulator, every widget, the
   lifecycle, the JVM natives, sensor plumbing, and the registry guards all
   come for free.

## AMENDMENTS

*(append here as execution diverges; amendments override the body above)*

### A1 — Stages 4b and 4c merged (Stage 4b)

The body plans 4b (LVGL engine) and 4c (widgets + Java-binding layer) as
separate green stages. They landed as one commit because the dependency
graph does not permit the split: `view_ops` calls into the keyboard and
number-picker widgets, and those widgets call back into the engine. Splitting
them would have required a `picodroid-core` → `platforms/rp` call, which is a
circular crate dependency.

The Java-binding layer (`view`, `display`, `assets`, `fields`, `view_group`,
`widgets/`) moved in the same commit for a second reason: it is reached
through ~113 `pub(in crate::graphics)` functions. `pub(in ...)` paths cannot
span crates, so leaving the binding layer behind would have meant widening
all of them to `pub` — discarding the visibility discipline that documents
which functions are engine-internal.

### A2 — GC-root guard is shared, and covers both crates (Stage 4b)

§3.G specifies a guard that every `visit_*roots` definition "in both trees"
is registered. Stages 2b–4a only implemented the platform half; the
`picodroid-core` constant was asserted by nothing. That gap surfaced when
`graphics/display.rs` moved here in this stage: its provider was still
registered from the platform list (resolving through the
`pub use picodroid_core::graphics;` re-export, so it compiled and the root
was still visited), leaving the count wrong on both sides.

The scanner now lives in `test_support/gc_root_scan.rs` and is
`#[path]`-included by both crates' `gc_root_registration.rs` — one
implementation, two `src` roots, mirroring `build_support/board_cfg.rs`. Two
hand-maintained copies of a drift guard can themselves drift.

`test_support/` is a new top-level directory, parallel to `build_support/`:
non-crate Rust shared by `#[path]` include, but reached from `cfg(test)`
modules rather than build scripts.

Both failure paths are verified by deliberately breaking them, not by
inspection: dropping a uniquely-named provider, and dropping one of four
identically-named ones (`visit_checked_change_listener_roots`, defined in
switch/check_box/radio_button/toggle_button — the case where an unqualified
match would let one registration cover all four).

Residual blind spot, for Stage 9: a source scan sees text, so a registration
compiled out by a `cfg` still reads as present. None is cfg-gated today, but
a board-capability gate (`has_buttons`, say) would slip through. Closing it
is cheap — assert at the end of the platform's `register_all` that
`gc_roots::provider_count()` equals the sum of the two constants, which
checks what actually registered rather than what the source says. Deferred
only to keep this commit's scope to the move; it costs a little RP2040 flash
for the message, so land it with the flash delta measurement.

*2026-09-02:* **landed** — `platforms/rp/src/gc_root_registration.rs` asserts
`provider_count() == core EXPECTED + platform EXPECTED` after `register_all`,
with a comment naming this item.

### A3 — the Stage 9 shadow-twin guard needs an allowlist (Stage 4b)

§4 Stage 9 specifies a guard that fails if any relative path exists under
both `platforms/*/src/` and `picodroid-core/src/`. As written it would fail
today on two paths that are deliberate counterparts, not stale twins:

- `gc_root_registration.rs` — one list per crate is the design (§3.G).
- `hal/mod.rs` — core's is the trait/facade surface, the platform's is the
  rp-vs-sim routing. They share a name because they are two ends of the same
  seam.

So the guard needs an explicit allowlist of those two, each with a comment
saying why it is legitimate. A guard that has to be silenced by deleting it
teaches nothing; one that names its exceptions keeps the rule enforceable.

### A4 — `hardware/` defers to stage 6; five HAL functions added (Stage 5)

§4 Stage 5 lists `hardware/` with pio/net/os/util. It cannot go yet:
`sensors::drain_sensor_events` and `deliver_event` take
`&mut PicodroidNativeHandler` to invoke Java listener callbacks, while
`native_handler` dispatches into `sensors` — mutually dependent, so
splitting them across crates is a circular crate dependency. Same shape as
A1. `hardware/` moves with `native_handler` in stage 6; stage 5 is
net/os/pio/util.

Moving `pio` surfaced five HAL functions the seam did not carry:
`i2c::{write,read}`, `spi::{transfer,write}` and `uart::reconfigure`. They
differ from the `_slice`/`_raw` pairs already in the traits by taking
`&ArrayHeap` and a heap index rather than a borrowed slice — the natives
transfer straight out of a Java array, letting the platform bounds-check and
copy in one place.

None of the five appears in `hal/contract.rs`'s 72 assertions. That is the
second gap found by converting the v1 doc-block contract into traits, after
`udp_sendto`/`udp_recvfrom` in stage 2a: the assertions were hand-written
and drifted from the surface actually in use, whereas a trait bound cannot.
Evidence for retiring the trait-covered assertions in stage 9 rather than
keeping both.

### A5 — stages 6 and 7 merged; the `system/` shim tree deleted (Stage 6)

Third forced merge, and the clearest. `native_handler` reaches into
`lifecycle`, `service_lifecycle` and `app` (8 references); those three reach
back into `native_handler` 47 times. The thin direction could have been cut
with function-pointer hooks, but both sides end up in picodroid-core
regardless, so that indirection would have been permanent structure bought
to split one commit. They moved together.

Landed in two commits: the `HalFs` seam alone (stage 6a), then the move.

**`crate::system::` is gone entirely.** The plan assumed re-export shims at
the old paths, as earlier stages used. Once `native_handler` moved, only
seven platform call sites still referred to `crate::system::…`, and the
dispatch arms that motivated path preservation now live in picodroid-core
pointing at `crate::graphics` directly. A shim tree inside the crate that no
longer owns the code is cargo-culting, so those seven were repointed at
`picodroid_core::…` and `platforms/rp/src/system/` deleted.

**A double-registration bug, caught by a dead-code warning.** Moving
`run_jvm_with` into `boot::run_app` silently rerouted GC-root registration:
`run_app` called *picodroid-core's* `register_all`, so the platform's — which
held the idempotence latch — became unreachable. `run_app` re-runs on PDB app
reload, core's `register_all` had no latch, and 17 providers registered twice
exceeds `MAX_PROVIDERS = 32`, which asserts. The fix is both halves: the latch
moved into core's `register_all`, and `PlatformHooks` gained
`register_gc_roots()` so a family's own providers are still reached. Core
registers its own first, so a family that gets the hook wrong loses only its
own providers, never the framework's. The method is required rather than
defaulted — an empty body is then a decision rather than an unasked question.

**BLD-02 reaches 0.** `native_handler::interrupted` was three cfg arms
(debug bridge on device, `false` without one, no override in sim); the
platform hook already draws exactly that line, so it collapsed to
`host::stop_requested()`. That was the last `not(feature = "family-rp")`
gate. `scripts/pre-commit` now expects zero and says why: a new occurrence is
shared code guessing at the platform instead of asking it.

`NativeHeapStats` was reshaped to the four values `mem_diag` actually prints
(`used_bytes`, `free_bytes`, `min_ever_free_bytes`, `largest_free_block`),
dropping `alloc_count`/`free_count`, which nothing read. Both arms of
`sample_native_heap` — FreeRTOS FFI on device, the sim allocator's byte meter
plus `heap_4` mirror — moved into the platform hook, so the monitor formats
one struct instead of reconciling two shapes.

`util/log.rs` keeps its own `feature = "sim"` arms rather than converting to
`pd_log`: it is the `picodroid.util.Log` native and its simulator arm writes
`[Tag] message` to **stdout**, which is what every example app emits and what
the sim-test harness greps. `pd_log` writes to stderr. Four error sites in
`boot.rs` keep hand-paired arms for a different reason — the papk error type
implements `Debug`/`Display` but not `defmt::Format`, so a `pd_error!` would
not compile on the device arm.

**Exactly one crate may drive the JVM.** The RP2040 flash gate caught this:
after the move the image overflowed FLASH by 24.6 KB, `.text` up 32.9 KB.
The cause was not the moved code — the modules roughly broke even, and
`graphics` shrank — but `pico_jvm`, up 35.5 KB. Symbol-level diff:
**23 duplicated symbols, 37.9 KB**, led by `pico_jvm::interpreter::execute`
codegen'd twice at 12.7 KB a copy. At the previous commit there was one
trivial duplicate.

The second instantiation was `bg_worker`, the background-pool worker loop.
It builds a `Jvm` and invokes bytecode, and it had stayed in the platform
crate because it needed the class loader and shared heap — which this stage
moved. With `codegen-units = 1` and no LTO, two crates instantiating the
interpreter get two copies of it. Moving `bg_worker` into picodroid-core
turned a 24.6 KB overflow into `.text` **4.2 KB smaller than before the
stage**, with duplicates back to one.

Two things follow. For the porting guide: a family crate must not drive the
JVM — it registers seams and hands off to `boot::run_app`. And LTO is not
the escape hatch; measured on this build, `lto = "thin"` costs a further
25 KB and `lto = "fat"` 13 KB over no LTO, so the existing
"LTO makes the image bigger" note survives the crate split.

After this stage `platforms/rp/src` is ~11.4k LOC, matching §5's predicted
end state and file list.

### A6 — the simulator HAL is shared (Stage 8)

Fourteen of the seventeen `hal/sim/` modules move to
`picodroid-core/src/hal/sim/`. `boot`, `flash` and `pdb_usb` stay: they stub
genuinely family-specific machinery (reset entry, XIP flash, the USB debug
bridge) that no shared simulator can stand in for. This is the step that
deletes the porting guide's "copy `hal/sim/`" instruction — the mechanism
that produced ESP's 17 drifting twins. `platforms/rp/src` drops to ~9.7k LOC
and no longer depends on `minifb` at all: a family crate should not need a
host GUI dependency to be simulatable.

Three things the move surfaced:

*Sibling calls, not seam round-trips.* `sim/display.rs` calls
`hal::touch::read_point` and `hal::gpio::inject`. Inside the platform crate
those were sibling calls within one sim HAL; in picodroid-core `crate::hal`
means the *facade*, so they would have gone out through the platform's
registration and straight back into these same functions. They are now
`super::touch::` / `super::gpio::`.

*The simulator wants pin-bearing config, and that is correct.* `sim/touch.rs`
emulates a real XPT2046 — CS line, SPI frequency, calibration — rather than
faking its outputs, so it needs the same `touch_config.rs` the hardware
driver does. `emit_touch_config` therefore moved into `build_support/` and is
now called by both build scripts into their own OUT_DIRs: one generator, two
consumers, the `board_cfg.rs` rule. `sim/display.rs` went the other way — it
had been including the pin-bearing `display_config.rs` for four geometry
constants, and now reads the neutral `board_cfg::display`, because a host
window has no backlight pin, reset pin or MADCTL.

*`cfg(test)` does not cross a crate boundary, again.* The platform's own
`cargo test` routes `mod chip` to the simulator HAL, but a dependency is
never compiled with the dependent's `cfg(test)`, so `picodroid_core::hal::sim`
did not exist there. Fixed with a `[dev-dependencies]` entry enabling
picodroid-core's `sim` feature for test builds — the third instance of this
same subtlety, after `HalFs` and the `native_handler` test shims.

### A7 — guards, docs and the final measurement (Stage 9)

**Flash delta, rp2040 (the constrained target), across the whole
extraction** — `74cf8c9` to the final commit:

| Section | Baseline | After | Delta |
|---|---|---|---|
| `.text` | 703,928 | 703,736 | **−192** |
| `.rodata` | 195,872 | 195,728 | **−144** |

−336 bytes against a budget of ≤ ~2 KB. Worth being clear about why that is
not luck: without the `bg_worker` fix in A5 the same tree was +38 KB and
would not link on rp2040 at all.

**`contract.rs`: 130 lines to 50.** Only `boot`, `flash` and `pdb_usb`
remain, which have no traits because they have no shared counterpart to form
a contract with. Two things the retired assertions covered are checked
elsewhere rather than dropped — the display constants by the drift assertion
in `hal/mod.rs`, the family `gpio` enums by `glue.rs`'s converters. The
117-line v1 doc-block above them is now a pointer at the traits.

**Shadow-twin guard** in `scripts/pre-commit`, with an allowlist for the
three real seam pairs A3 identified (`gc_root_registration.rs`,
`hal/mod.rs`, `hal/sim/mod.rs`). Verified by planting a stale copy.

**`littlefs-rust` dropped from picodroid-core** — unused once `HalFs` moved
the LittleFS body to the platform, so the seam paid for itself twice.

Docs: ARCHITECTURE.md's module map is split by crate and its boundary table
gained the three rules this work established (a platform crate must not
construct a `Jvm`; no same-path file in both trees; a native module holding
Java refs must register a root provider). The porting guide's "copy
`hal/sim/`" instruction is replaced by a section explaining why not.

Still open, deliberately: `docs/parity-audit.md` records that the simulator
still does not take real recursive monitors. The RTOS seam makes that a
one-line change now, but it is a behaviour change rather than a move, so it
stays out of this work.

### A8 — closing pointer (2026-08-14)

The residual family-neutral extraction this doc's §5 predicted was measured
and continued in its direct successor, `docs/designs/family-neutral-residue.md`
— stages 1–5 of that plan are executed as of 2026-08-01, with its own
amendment trail; that doc is where the remaining work lives. And A7's "still
open, deliberately" item is no longer open: the simulator takes real recursive
monitors now, because it runs the real FreeRTOS kernel — resolved by
`docs/designs/freertos-host-sim.md` (`e44b879`).
