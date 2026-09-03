# Design: the porting seam — one checklist, and the last neutral code out of `platforms/rp`

> Produced 2026-09-02/03 by a planning session (three parallel audits over
> the non-HAL family files, the `hal/rp/**` tree plus the build layer, and the
> core seam plus every porting document; then a design pass). Every claim was
> checked against source at `80aad79`. Amendments are appended at the bottom
> and OVERRIDE the design body where they conflict. Execute from this doc;
> append an amendment when reality diverges.
>
> Direct successor to `docs/designs/family-neutral-residue.md`, whose stages 6
> and 7 were never executed. Its vocabulary is assumed: HAL CONTRACT v2 (the
> traits in `picodroid_core::hal`), `Rtos`, `PlatformHooks`, the `set_*!`
> registration macros, `register_sim_platform!`, the shadow-twin rule, and its
> decisions D1–D8. "Seam" below means the boundary between `picodroid-core`
> and a family crate.

## 0. Why this exists

Two problems, one cause.

**A port has no single list of what it must provide.** Today a new family
must satisfy seven different kinds of obligation: trait + registration macro
(101 `__pd_*` link symbols), generic trait parameters (the debug bridge and
installer), simulator function pointers (`register_sim_platform!`,
`BootLeaves`), `cfg` flags its `build.rs` emits, linker and C symbols,
`build.rs` outputs, and `board.toml` keys. They are spread over about ten
files in `picodroid-core` and are written down together nowhere. The porting
guide on the website describes a tree that no longer exists (`hal/sim/` with
"three stubs"), functions that moved (`start_tasks`, the `pdb_usb` and
`flash.rs` free functions), and never names the four PDB/install traits, the
filesystem trait, the simulator registration, the `Rtos` trait, or the
platform hooks. One of its tables is actively wrong: `[background_pool]
priority` is documented as "default 5, range 1..=10" while
`build_support/board_cfg.rs:308-312` panics unless it is 15. A porter who
follows the guide cannot build.

**Family-neutral code is still in the family.** The predecessor's Stage 6
("misc HAL sweep") and Stage 7 ("guards, docs, measurement") were written on
2026-07-27 and never run. A fresh audit of `platforms/rp/src` (8,774 lines at
`80aad79`) finds about 1,240 lines under `hal/` and about 500 outside it
that a second family would copy word for word. Three are already twins with
the shared simulator: the `Pull`/`EdgeTrigger`/`GpioEvent` enums (defined in
three places, converted between in `glue.rs`), the scripted-touch override
(`hal/rp/touch.rs:16-45` and `hal/sim/display.rs:73-76, 307-330`, the same
three atomics), and the GPIO event ring (`hal/rp/gpio.rs:329-419` against
`hal/sim/gpio.rs:96-119`). Two constants are hand-copied: the USB
vendor/product ID lives in both `tools/pdb/src/devices.rs:10-11` and
`hal/rp/pdb_usb/protocol.rs`. Nine unit tests in `hal/rp/adc.rs` and
`hal/rp/pwm.rs` have never compiled (no host `#[path]` shim), and the pwm
ones assume 125 MHz while the default board runs at 150 MHz.

The cause is the same as the predecessor's §0: what was "predicted to be
family-specific" was never measured against a second family, because there is
none. This doc measures it against the question a porter asks — *would I copy
this?* — and writes the answer down as code (`picodroid_core::porting`) rather
than prose, with a test that keeps the code and the guide in step.

**The acceptance test is unchanged** — `shared-core-extraction.md` §5's "what
a future family must provide". This work shrinks that list and makes it
machine-checked.

## 1. Scale

### 1.1 Family-neutral residue in `platforms/rp/src/hal/` (6,013 LOC)

| # | Where | Neutral LOC | What it is |
|---|---|---:|---|
| H1 | `hal/rp/gpio.rs:61-66, 110-115, 301-311` + `hal/sim/gpio.rs:39-44, 69-74, 80-89` + `glue.rs:82-88, 97-115` | ~45 | Duplicate `Pull`/`EdgeTrigger`/`GpioEvent` and the converters between them |
| H2 | `hal/rp/gpio.rs:329-419` (+ `hal/sim/gpio.rs:91-119`) | ~90 | The GPIO event ring, drop tally, overflow warning |
| H3 | `hal/rp/touch.rs:16-45, 119-128` (+ `hal/sim/display.rs:68-76, 303-336`) | ~40 | The scripted-touch override state machine |
| H4 | `hal/rp/i2c/mod.rs:646-680`, `hal/rp/spi/mod.rs:449-570` (+ sim twins) | ~155 | The Java-array ↔ slice wrappers behind `HalI2c::{write,read}` / `HalSpi::{transfer,write}` |
| H5 | `hal/rp/pdb_usb/protocol.rs` | 148 | USB CDC descriptors; VID/PID also hand-copied in `tools/pdb` |
| H6 | `hal/rp/flash.rs:42-49, 71-82, 252-282` + `packagemanager/mod.rs` | ~65 | The PAPK slot layer over two flash primitives |
| H7 | `fs/storage.rs:32-37, 46-58, 72-74` | ~25 | Block bounds / program-alignment checks, twins of `fs/storage_host.rs:80-83, 105-113, 122` |
| H8 | `i2c/protocol.rs`, `spi/protocol.rs`, `adc.rs:66-88`, `pwm.rs:123-169`, `build.rs:47-54` | — | Misnamed files, nine dead tests, hand-derived `repo_root`/`is_embedded` |
| H9 | `hal/mod.rs:2-3, 65-73, 85-87, 91`, `hal/contract.rs:26-27` | — | Comments that describe a tree that no longer exists |

Networking (`net.rs` 381, `wifi_task.rs` 168, `pio_spi.rs` 606, `trng.rs`
114; `NetworkInterface_CYW43.c` 224, `net_init.c` 151, `cyw43_port.c` 377,
`libc_str.c` 43) is neutral-for-FreeRTOS+TCP but **excluded** — see E8.

### 1.2 Family-neutral residue outside `hal/` (1,137 code LOC)

| # | Where | Neutral LOC | What it is |
|---|---|---:|---|
| N1 | `boot_budget.rs:176-185, 206-383` | ~185 | The simulator boot-budget accounting engine (ledger, `black_box`, precharge, report, charge/release) |
| N2 | `main.rs:58-93, 146-181`, `glue.rs:734-787` | ~100 | The forwarding `#[global_allocator]`, the sim `main` body, `run_sim` and the sim/test cfg fork |
| N3 | `task_affinity.rs:120-253, 399-430` | ~165 | Source-scan plumbing (walker, comment stripper, arg parser, `#define` asserts); the test also scans `picodroid-core/src` from inside the family |
| N5 | `boot_tasks.rs:52-66` = `sim_boot.rs:69-95` | 15 | The heap-atomic-hooks block, verbatim twice, "keep in lockstep" unenforced |

`app.rs`, `pdb/{mod,platform,coordinator,pending}.rs`, `packagemanager/flash/`,
`fs/mod.rs`, `boards/`, `gc_root_registration.rs`, the device half of
`main.rs`, `boot_tasks.rs`'s topology and supervisor loop, `task_affinity`'s
rules: correctly placed, no move.

### 1.3 Documentation debt

Porting guide: 11 stale claims, 16 missing topics (full list in §7).
`ARCHITECTURE.md`: "three genuine seam pairs" (there are four); neither of the
predecessor's two §7 boundary rules was added; zero mentions of
`register_sim_platform!`, `set_rtos!`, `set_platform_hooks!`, `BootLeaves`,
`FsBackingStore`, `littlefs`. `shared-core-extraction.md:479-480` items 4–5
predate the four traits. `family-neutral-residue.md` §3.I never landed, its
§6 numbers are 2–3× stale, and its Stage 6 lists a deletion that was already
done.

## 2. Decisions

Confirmed with the maintainer before execution. D1–D8 of the predecessor stand.

### E1 — `picodroid_core::porting` is the canonical checklist

One module re-exports every item a family implements, and its module doc *is*
the "what a port provides" list, grouped by mechanism in the order a bring-up
meets them. A workspace test scans the seam files for every `pub trait` and
every `#[macro_export]` macro and fails if one is not named in `porting.rs`
and in the porting guide. The guide points at the module rather than
restating signatures, so it cannot fall behind the way the v1 doc-block did.

### E2 — The simulator adopts the same event ring as the device

The predecessor's §3.I said the simulator keeps its `Mutex<VecDeque>` because
"cross-thread injection is a genuinely different concurrency contract". Two
facts argue otherwise. The device ring already has two writers — the
`IO_IRQ_BANK0` ISR and `inject` from the PDB task (`gpio.rs:347-378`) —
with no protection between them, so the contract was never single-writer. And
the simulator's queue is unbounded while the device drops after 63 edges, so
a stalled simulator never reports the overflow a stalled device would —
`parity-audit.md` TCH-01 describes the two as "the same queue" and they are
not. One type: `GpioEventRing<64>` bare on the device (with `inject` under
`cortex_m::interrupt::free`, which also closes the race), and
`Mutex<GpioEventRing<64>>` in the simulator to serialise its two producers.

### E3 — The Java-array wrappers become default trait methods

`HalI2c::{write,read}` and `HalSpi::{transfer,write}` gain default bodies
over the slice methods, through a shared `hal::array_io` helper with a
64-byte staging cap and an overridable associated const. A porter writes the
four slice functions per bus and nothing else. The shims already dispatch
`<T as HalI2c>::write`, so a default resolves at the registration site with
no facade or macro change. Flash is predicted at or below zero (the RP SPI
copies drop two duplicated ISR-transfer blocks); it is measured in S5, and
the recorded fallback is "keep the helper, drop the defaults" if it grows by
more than ~200 B.

### E4 — USB identity goes to `pdb-protocol`; descriptors to core

`tools/pdb/src/devices.rs:10-11` hand-mirrors VID 0x1209 / PID 0xCDC0 — the
exact defect D1 exists to remove. The identity (VID, PID, manufacturer and
interface strings) moves to `pdb_protocol::usb`, and the host tool imports
it. The CDC descriptor set moves to `picodroid_core::pdb::usb_cdc`, built
with `const fn` from the protocol constants, as a *reference* set: the
endpoint layout is family-influenced, so a family with a different USB stack
may build its own `CONFIG_DESC` but must still take the identity from the
protocol crate.

### E5 — A `PapkSlot<F>` adapter; the supervisor loop stays (D4 reaffirmed)

A family supplies three constants (meta offset, max data size, sector size),
two flash primitives (`erase_range`, `program_range`) and a reset; core
implements `PapkFlash` over them — sector round-up plus the meta sector on
erase, page bounds on write, `flash_image::build_meta_page` programmed last
on commit — and the memory-mapped read of an installed PAPK. The `unsafe`
contract (erase and program only while the JVM core is parked) moves onto
the primitives trait verbatim.

The JVM supervisor loop (`boot_tasks.rs:159-200`) and the child-task
bookkeeping in `pdb/pending.rs` stay family-side. D4's reason is unchanged —
the park half encodes this family's flash topology on a sample size of one —
and the code was hardened on 2026-08-30 with subtle ordering (count before
spawn; the child registers itself). Two things happen here and no more: the
heap-atomic-hooks block becomes one shared function (E7), and the loop's
obligations become a checklist in the porting guide with `boot_tasks.rs` as
the reference implementation.

### E6 — The boot-budget engine moves to core; the model stays family data

The ledger, the `black_box` that keeps LLVM from eliding a never-freed
allocation, precharge, report and the charge/release pair move into
`picodroid_core::hal::sim::boot_budget`. What stays is data: the stack
constants, `TCB_EST_BYTES`, `QUEUES_MISC_BYTES`, the `BOOT_TASKS` table, and
`default_stack_bytes`, gathered into one `static MODEL: BootBudgetModel`. The
model crosses as a `register_sim_platform!` parameter — D6's pattern — and
the macro now also generates the simulator's `main`, so `BootLeaves`,
`glue::run_sim` and the sim/test cfg fork in `glue.rs` all go away.
`PlatformHooks` is unchanged. A consequence to document: the family's
simulator Cargo feature must be named `sim`, because the generated `sim_main`
is gated on it in the expanding crate (the scripts already assume this).

### E7 — `rtos::freertos` is the one FreeRTOS-naming module in non-sim core

Both consumers of the heap-atomic hooks are FreeRTOS (`boot_tasks.rs` on the
device, `sim_boot.rs` on the host). Routing scheduler-suspend through the
`__pd_rtos_*` facade would add one call on every `AtomicSection` — the
hottest path in the JVM — for the sake of a family that does not exist. So
core gains `picodroid_core::rtos::freertos::install_heap_atomic_hooks()`, a
raw-`extern "C"` function a non-FreeRTOS family simply never calls. The new
seam guard (E10) allowlists exactly this file by path, so the exception is
written down rather than silently permitted.

### E8 — Networking stays family-owned; re-measured, not moved

The predecessor's Phase N was written against ~820 lines; the 2026-08 WiFi
bring-up made it ~1,300 lines of Rust and ~800 of C, spanning the board
schema, the C build and a new cfg channel, and it pays nothing until a second
*networking* family exists. Decided with the maintainer: record the numbers
(§1.1), document networking in the porting guide as "family-owned today", and
leave the design to its own doc when that family appears.

### E9 — `declare_family_hal!` is deferred

`hal/mod.rs:46-89` is thirty lines of `pub use` that either compile or do
not. A macro would hide the one piece of routing a porter most needs to see
— which module answers for which peripheral, and which are device-only. The
comments there are fixed instead (H9).

### E10 — Every new invariant is a workspace test

No new pre-commit lanes. `TWIN_ALLOW` keeps its four entries, BLD-02's
expected count stays 0, the size ratchet does its job. The new tests are
source scans in the repo's established shape (`gc_root_scan.rs`), sharing one
walker in `test_support/source_scan.rs`, and every scan pins a count so an
empty scan cannot pass — the lesson the predecessor recorded three times.

## 3. Seams

### 3.A `picodroid_core::porting`

`picodroid-core/src/porting.rs`, always compiled, re-exports cfg'd like their
sources. Its doc comment is the list below; the body is `pub use` lines.

1. **Trait + registration macro** (link-time `__pd_*` symbols): the eleven
   HAL traits and `hal::types`; `set_hal!` (nine buses), `set_hal_fs!`,
   `set_hal_net!`; `Rtos` with `TaskKind`, `TaskSpec`, `Timeout`, `Raw*`
   and `set_rtos!`; `PlatformHooks` with `NativeHeapStats` and
   `set_platform_hooks!`.
2. **Generic parameters** (no link surface): `PdbTransport`, `SysmonSource`
   (`SysmonSample`, `TaskSample`, `MAX_TASKS`), `CoreCoordinator`,
   `PapkFlash` — or `PapkSlotFlash` + `PapkSlot` + `read_mapped` —
   `InstallTransport`, handed to `run_pdb_task`.
3. **Simulator**: `register_sim_platform! { gc_roots, boot_budget, run_app }`,
   `declare_sim_global_allocator!`, `BootBudgetModel`, `BootTask`.
4. **Filesystem**: the `littlefs` feature, `FsBackingStore`, `FsGeometry`,
   `init_device`, `spawn_worker`, `set_hal_fs!(LittleFsHal)`; or a family's
   own `HalFs`.
5. **`cfg` flags** a family's `build.rs` emits through
   `build_support::board_cfg`: `has_display`, `has_touch`, `has_buttons`,
   `has_network`, `network_<type>`, `any_sensor`, `sensor_<kind>`.
6. **Link and C**: `FreeRTOSConfig.h`, the kernel port, the
   `vApplication*Hook`s, `__fs_start`/`__fs_end` (with `littlefs`), the
   `.papk_flash_init` section. LVGL is compiled by core's `build.rs`, never
   by a family.
7. **Data and discipline**: `board.toml`, `mcus/<chip>.toml`,
   `EXPECTED_PROVIDERS`, the boot-budget model, `TaskKind` → stack/affinity
   policy (`FsWorker` must be pinned on a family that runs from the flash it
   writes), the supervisor-loop checklist, `boot::run_app`.

Also re-exported, as helpers a porter may reuse: `hal::array_io`,
`GpioEventRing`, `TouchOverride`, `CORE_EXPECTED_PROVIDERS`.

Two tests in the module: `checklist_is_complete` (every `pub trait` /
`pub unsafe trait` in `hal/traits.rs`, `rtos/mod.rs`, `host.rs`, `pdb/*.rs`,
`install/*.rs`, `fs/mod.rs`, and every `#[macro_export]` macro in core, is
named in `porting.rs`; pinned count) and `porting_guide_names_every_seam`
(the same names appear in the website's porting guide).

### 3.B `hal::event_ring::GpioEventRing` (H2)

```rust
/// One producer context (an ISR, or a task that has masked it) and one
/// consumer task. Holds `N - 1` edges. Timestamps and wake-ups stay with
/// the caller: the ring knows neither the family's timer nor its semaphore.
pub struct GpioEventRing<const N: usize> { /* UnsafeCell<[GpioEvent; N]>, head, tail, dropped, reported */ }

impl<const N: usize> GpioEventRing<N> {
    pub const fn new() -> Self;
    /// `false` = full; the edge is dropped and tallied.
    pub fn enqueue(&self, pin: u8, rising: bool, t_us: u32) -> bool;
    /// Oldest edge. Warns once per change of the drop tally, through `pd_log`.
    pub fn drain(&self) -> Option<GpioEvent>;
    pub fn has_pending(&self) -> bool;
    pub fn dropped(&self) -> u32;
}
```

Counters are plain load/store atomics with acquire/release pairing — no
compare-and-swap, because thumbv6m has none. Deviation from §3.I recorded:
`take_drop_report` is folded into `drain`, so the warning text lives in core
and no family writes it.

### 3.C `hal::touch_override::TouchOverride` (H3)

```rust
pub enum OverrideSample { Inactive, Pressed(u16, u16), Lifted(u16, u16) }

pub struct TouchOverride { /* active, pressed, packed (x << 16) | y */ }
impl TouchOverride {
    pub const fn new() -> Self;
    pub fn inject(&self, x: u16, y: u16);   // press or drag-move
    pub fn release(&self);                  // lift; keep reporting the last point
    pub fn clear(&self);                    // disengage; real sampling resumes
    pub fn sample(&self) -> OverrideSample;
}
```

Device `read_point`: `Inactive` → panel, `Pressed(x, y)` → `Some((x, y))`,
`Lifted(..)` → `None`. Simulator `mouse_state`: `Inactive` → mouse,
`Pressed(x, y)` → `(true, x, y)`, `Lifted(x, y)` → `(false, x, y)`.

### 3.D `hal::array_io` and the trait defaults (H4)

```rust
pub const STAGING_CAP: usize = 64;
pub fn i2c_write(max: usize, arrays: &ArrayHeap, data_idx: u16, len: usize, write_slice: impl FnOnce(&[u8]) -> i32) -> i32;
pub fn i2c_read(max: usize, arrays: &mut ArrayHeap, buf_idx: u16, len: usize, read_slice: impl FnOnce(&mut [u8]) -> i32) -> i32;
pub fn spi_transfer(max: usize, arrays: &mut ArrayHeap, tx_idx: u16, rx_idx: u16, len: usize, transfer_raw: impl FnOnce(&[u8], &mut [u8])) -> i32;
pub fn spi_write(max: usize, arrays: &ArrayHeap, data_idx: u16, len: usize, write_raw: impl FnOnce(&[u8])) -> i32;
```

`HalI2c` and `HalSpi` each gain `const JAVA_XFER_MAX: usize = STAGING_CAP;`
and default bodies for the four array methods over their slice siblings.
Contract: `-1` when `len > JAVA_XFER_MAX`, `0` for `len == 0`, else the
slice call's result; `i2c_read` copies back `result.min(len)` bytes. An
associated const rather than a const generic, because `[u8; Self::N]` in a
default method is rejected on stable.

### 3.E `pdb_protocol::usb` and `picodroid_core::pdb::usb_cdc` (H5)

```rust
// pdb-protocol/src/usb.rs
pub const VID: u16 = 0x1209;   // pid.codes open-source VID
pub const PID: u16 = 0xCDC0;   // picodroid's allocation
pub const MANUFACTURER: &str = "Picodroid";
pub const PRODUCT: &str = "Picodroid";
pub const INTERFACE: &str = "PDB (USB CDC)";

// picodroid-core/src/pdb/usb_cdc.rs — the reference CDC-ACM descriptor set
pub const DEVICE_DESC: [u8; 18];  pub const CONFIG_DESC: [u8; 67];
pub const STR0: [u8; 4]; pub const STR1: [u8; 20]; pub const STR2: [u8; 28];
pub const LINE_CODING: [u8; 7];
```

### 3.F `install::slot` (H6)

```rust
/// # Safety
/// `erase_range` / `program_range` run only while the JVM core is parked
/// (the `CoreCoordinator` contract). `PapkSlot` inherits `run_install`'s park.
pub unsafe trait PapkSlotFlash {
    const META_OFFSET: u32;      // flash-relative offset of the boot-meta sector; data follows
    const MAX_DATA_SIZE: usize;  // slot size minus the meta sector
    const SECTOR_SIZE: usize;
    unsafe fn erase_range(flash_offset: u32, len: usize);
    unsafe fn program_range(flash_offset: u32, data: &[u8]);   // offset, len multiples of 256
    fn reset() -> !;
}
pub struct PapkSlot<F: PapkSlotFlash>(PhantomData<F>);
impl<F: PapkSlotFlash> PapkSlot<F> { pub const fn new() -> Self; pub const DATA_OFFSET: u32; }
unsafe impl<F: PapkSlotFlash> PapkFlash for PapkSlot<F> { … }
/// # Safety: `slot_base` maps `META_SIZE + max_data_size` readable bytes and
/// no erase/program is in flight (pre-scheduler, as `main` calls it).
pub unsafe fn read_mapped(slot_base: *const u8, max_data_size: usize) -> Option<&'static [u8]>;
```

### 3.G `fs::FsGeometry` helpers (H7)

```rust
impl FsGeometry {
    pub fn resolve(&self, block_count: u32, block: u32, offset: u32, len: usize) -> Result<u64, FsError>;
    pub fn check_prog(&self, offset: u32, len: usize) -> Result<(), FsError>;
}
```

### 3.H `hal::sim::boot_budget`, `sim_boot::main`, the macro (N1, N2)

```rust
pub struct BootTask { pub name: &'static str, pub stack_bytes: u32, pub sim_real: bool }
pub struct BootBudgetModel {
    pub tasks: &'static [BootTask],
    pub tcb_bytes: u32,
    pub queues_misc_bytes: u32,
    pub default_stack_bytes: fn(TaskKind) -> u32,
}
impl BootBudgetModel { pub fn modeled_boot_bytes(&self) -> u32; pub fn stack_bytes(&self, spec: &TaskSpec) -> u32; }
pub fn precharge(model: &BootBudgetModel);
pub fn report(model: &BootBudgetModel);
pub fn charge_task_spawn(model: &BootBudgetModel, spec: &TaskSpec) -> u32;
pub fn release_task_spawn(model: &BootBudgetModel, spec: &TaskSpec);

// registration
picodroid_core::register_sim_platform! {
    gc_roots    = crate::gc_root_registration::register_all,
    boot_budget = crate::boot_budget::MODEL,
    run_app     = crate::app::run_jvm,
}
// generates SimPlatform (Rtos + PlatformHooks), set_rtos!, set_platform_hooks!,
// and, under #[cfg(feature = "sim")]: pub fn sim_main()

// sim_boot.rs
pub fn main(model: &'static BootBudgetModel, run_app: fn());   // arm, precharge, fs, run, banner

// hal/sim/allocator.rs
macro_rules! declare_sim_global_allocator { () => { /* forwarding newtype + #[global_allocator] static */ } }
```

### 3.I `rtos::freertos` (N5)

```rust
#[cfg(not(test))]
pub fn install_heap_atomic_hooks();   // vTaskSuspendAll / xTaskResumeAll into pico_jvm::atomic_section
```

### 3.J `test_support/source_scan.rs` and the core seam guard (N3)

```rust
pub fn sources(dir: &Path, exts: &[&str], skip_file: Option<&str>, out: &mut Vec<PathBuf>);
pub fn strip_comments(text: &str) -> String;
pub fn read_stripped(path: &Path) -> String;
pub fn rel(root: &Path, path: &Path) -> String;
```

`rtos/mod.rs` gains `#[cfg(test)] mod seam_guard`: non-sim core (minus
`hal/sim/**` and `rtos/freertos.rs`) names no RTOS primitive outside the
`Rtos` trait — the regex `\b[vx](Task|Queue|Semaphore|Timer|Port)[A-Z]\w*`,
plus `freertos_rust`, `Task::new()`, `.core_affinity(`, `CurrentTask::`,
`std::thread::spawn`, `thread::Builder`. Pinned file count.
`task_affinity.rs` stops scanning `picodroid-core/src`.

## 4. Stages

Every stage ends green: `./scripts/pre-commit` printing
`==> All checks passed.`, plus `./scripts/sim.sh --app helloworld`,
`--app benchmark`, `--app gcstress`, and
`perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky`. Every stage
that touches device code records the rp2040 release `.text`/`.rodata` delta
as an amendment; cumulative budget ≤ ~2 KB, as before. Baseline: `80aad79`.

| # | Stage | Scope | Device code | HIL |
|---|---|---|---|---|
| S0 | Design doc | This file; B17 on the predecessor. | — | — |
| S1 | GPIO types + `hal/mod.rs` comments | H1, H9 | types only | no |
| S2 | Simulator boot consolidation | N1, N2 | no | no |
| S3 | USB identity + descriptors | H5 | rodata only | `pdb ping` (batched into S6) |
| S4 | Renames, adc/pwm tests, `build_support::config` | H8 | rename only | no |
| S5 | Java-array wrappers as trait defaults | H4 | yes | optional |
| S6 | Event ring + touch override | H2, H3 | yes | **yes** — buttons, touch, `pdb input` |
| S7 | Flash slot + fs geometry | H6, H7 | yes | **yes** — install both chips, `STATUS_INCOMPAT`, `bootcount` |
| S8 | Shared hooks + source scan + seam guard | N5, N3 | yes (−bytes) | no |
| S9 | `picodroid_core::porting` | E1 | no | no |
| S10 | Docs, ARCHITECTURE, closing measurement | §7 | no | — |

Lowest device risk first. S6 and S7 are adjacent so one bench session covers
both. S9 and S10 land together: S9's guide test needs S10's guide.

### S1 — GPIO types, honest comments

Delete the family's and the simulator's local enums; both `use` /
`pub use` `crate::hal::types`. Delete `glue.rs`'s converters and the
`GpioEvent` field copy. Fix the four stale comments in `hal/mod.rs` and the
sentence in `contract.rs`. Flash: expect 0.

### S2 — Simulator boot in one place

§3.H. The ledger moves word for word. The family's `boot_budget.rs` keeps
constants, `default_stack_bytes` and `MODEL`. `main.rs`'s sim `main` becomes
`glue::sim_main()`; its allocator newtype becomes one macro call. Drop the
charge call on the refused `JvmChild` in `hal/sim/rtos.rs` (it charged an
arena that does not exist in that build). Verify with a capped run
(`./scripts/sim.sh -l 200 --app helloworld`) and the banner reconciling
exactly; `cargo test -p picodroid` proves the platform test build needs no
cfg fork. Flash: byte-identical.

### S3 — USB identity in one place

§3.E. Delete `hal/rp/pdb_usb/protocol.rs`, its `#[path]` shim, and the
`hal/sim/pdb_usb.rs` stub; gate `pub use chip::pdb_usb` device-only;
`assemble_u32_le` → `u32::from_le_bytes`. Flash: rodata byte-identical.

### S4 — Names, dead tests, build helpers

`i2c/protocol.rs` → `i2c/timing.rs`; `spi/protocol.rs` → `spi/xfer.rs`
(deviation from Stage 6's `spi/regs.rs`: the file's content is the live
transfer state, not registers). `pwm.rs` → `pwm/{mod,math}.rs` with
`clock_params(pclk_hz, freq_hz)` and `duty_to_cc`; `adc.rs` →
`adc/{mod,math}.rs` with `raw_to_volts`; two new `#[path]` shims.
`build_support/config.rs::{repo_root, is_embedded}` used by both build
scripts. Flash: 0.

### S5 — Java-array wrappers as defaults

§3.D. Delete the four hand-written copies on each side. Behaviour notes: RP
SPI Java transfers use two 64-byte stack buffers instead of the static
staging buffer; the simulator's Java `I2cDevice.read` now answers through
`read_slice` (the BME688 fake) instead of zeros. Flash: predicted ≤ 0;
fallback per E3.

### S6 — Event ring and touch override (HIL)

§3.B, §3.C. `inject` on the device goes under `interrupt::free` (a real race
closed in passing — record it). Tests: order, capacity `N - 1` (sabotage with
`N`), wrap, drop tally, warn-once; the override state machine. HIL on both
chips: real buttons and touch, `pdb input tap/swipe/keyevent`, idle-sleep
wake, and S3's `pdb ping`. Run the parity harness's scripted-input suites
once, since the simulator queue is now bounded. Flash: expect ±50 B.

### S7 — Flash slot and fs geometry (HIL, two commits)

7a: §3.G, both storage impls call the helpers; RP's `geometry()` override and
its two constants go. 7b: §3.F; `flash.rs:252-282` deleted;
`read_flash_papk` = one `read_mapped` call; `packagemanager/mod.rs` =
`RpFlash: PapkSlotFlash` + `type RpPapkFlash = PapkSlot<RpFlash>`; the three
`debug_assert!` at `flash.rs:45-47` become one `assert!` with a
`&'static str`. Tests: a recording mock over erase rounding, page bounds,
commit page; `PapkSlot<Mock>` through the existing orchestrator tests
(sabotage: swap data/meta order). HIL on both chips: three clean installs, one
`STATUS_INCOMPAT` with the old app still running, `bootcount` across two
flashes. If the rp2040 install/bootcount hang of
`docs/bugs-rp2040-flash-2026-08-01.md` reproduces, record "no regression
versus the pre-change build" as B15 did. Flash: expect ≈ 0.

### S8 — Shared hooks, shared scanner, seam guard

§3.I, §3.J. `rtos.rs` → `rtos/mod.rs`. Sabotage both scans before committing.
Flash: expect a few bytes fewer.

### S9 / S10 — The porting module and the docs

§3.A and §7.

## 5. End state — what stays in `platforms/rp`

- `main.rs` — entry, exception handlers, FreeRTOS hooks, the device
  `#[global_allocator]`, two simulator macro calls, four `#[path]` test shims.
- `glue.rs` — HAL impls (four fewer methods), the device `Rtos` and
  `PlatformHooks`, the registrations, one `register_sim_platform!`.
- `boot_tasks.rs` — task topology and the supervisor loop (D4's reference
  implementation), calling the shared hooks installer.
- `boot_budget.rs` — stack constants, `default_stack_bytes`, `MODEL`.
- `task_affinity.rs` — `spawn`, the core masks and allowlist, the RP-side
  scans over the shared helpers.
- `pdb/{pending,coordinator,platform,mod}.rs` — the park handshake, the CDC
  transport, the FreeRTOS sysmon source.
- `packagemanager/` — `RpFlash: PapkSlotFlash`, the `.papk_flash_init`
  section.
- `fs/{mod,storage}.rs` — the linker symbols and the backing store over
  `FsGeometry`.
- `hal/rp/**` — the silicon: flash primitives and `with_xip_disabled!`,
  `core1_park`, DMA, PIO gSPI, TRNG, the USB DPRAM driver, `i2c/timing.rs`,
  `spi/xfer.rs`, `pwm/math.rs`, `adc/math.rs`, networking (E8), the C port
  shims.
- `hal/{mod,contract}.rs`, `gc_root_registration.rs`, `app.rs`, `boards/`,
  `mcus/`, `build.rs`.

Expected: `platforms/rp/src` from 8,774 lines to about 7,800.

## 6. Deferred, with reasons

- **Networking** (E8): family-owned; own design doc when a second networking
  family exists. Note `pio_spi.rs:272-273` uses `debug_assert!` on a device
  path — never runs; fix when that code is next touched.
- **`declare_family_hal!`** (E9).
- **`core1_park` handshake** (~70 neutral lines): every multicore family that
  runs from the flash it writes needs this shape, but there is one such
  family. Documented in the guide; revisit at second-family bring-up.
- **`spi_bus` chunking, `output_pin` optional-pin handling**: Stage 6's
  deferral stands (~40 lines; a seam costs more than it saves at n = 2).
- **`HalI2c::write`'s `address: u32` against `write_slice`'s `u8`**: a
  contract-v3 question, not a porting-seam one.
- **The child-task registries** (three for one population: `pending.rs`,
  `hal/sim/rtos_freertos.rs`, `threads.rs`): tied to D4; revisit with it.

## 7. Documentation and guards

- **Porting guide** (`website/src/content/docs/reference/porting-guide.md`):
  rewritten around `picodroid_core::porting`. Keep the anchors other pages
  link: `#boardtoml-reference`, `#background_pool--optional-thread-pool-tuning`.
  Fix the actively wrong `[background_pool] priority` row (must be 15), the
  `network_type` "not parser-enforced" claim (it is), add `handle_slots` and
  `framework_class_excludes`, the D4 checklist (including "do not put a stop
  check in your HAL `sleep`"), the `scheduler_running` trap, the `FsWorker`
  pin rule, `pd_log`/defmt, `EXPECTED_PROVIDERS`, `TWIN_ALLOW`, "LVGL is
  core's build". Validate links with `npm run build` in `website/` (not in
  pre-commit).
- **`ARCHITECTURE.md`** (and its website mirror): four seam pairs; the two
  boundary rules from the predecessor's §7; module-map rows for `porting.rs`
  and `hal/sim/boot_budget.rs`; the platform table refreshed.
- **`docs/parity-audit.md`**: `boot_budget` path; TCH-01 (now literally the
  same type). **`shared-core-extraction.md:479-480`**, **`freertos-host-sim.md`**
  (`BootLeaves` is gone): one-line refreshes.
- **Guards**: all workspace tests (E10) — ring, override, array shims,
  `PapkSlot`, `FsGeometry`, the budget model, descriptor identity, the seam
  guard, the porting checklist, and the nine adc/pwm tests that now run.
  Pre-commit unchanged.

## AMENDMENTS

*(append here as execution diverges; amendments override the body above)*

### A1 — S1 landed: one set of GPIO types (2026-09-03)

`hal/rp/gpio.rs` and `hal/sim/gpio.rs` both use `picodroid_core::hal::types`
now; `glue.rs`'s two converters and its `GpioEvent` field copy are gone, and
the four stale comments in `hal/mod.rs` plus the sentence in `contract.rs`
say what the tree does. Net −14 lines across six files.

**Flash, rp2040 `--release` at the parent `80aad79`:** `.text` 697,776,
`.rodata` 175,552, `.data` 2,300. **After S1: byte-identical.** The enums
had the same layout on both sides, so the converters compiled to nothing
and deleting them changed nothing. That baseline is the one every later
stage measures against.

Verification: `./scripts/pre-commit` green (2m50s), helloworld / benchmark /
gcstress / blinky in the simulator. One note for the next reader: the
simulator embeds the app at build time, so the first `blinky` run after a
Rust change spends its whole `alarm 5` compiling and prints nothing; run it
once with a longer alarm, then the five-second form is meaningful.

### A2 — S2 landed: the simulator boots from one place (2026-09-03)

§3.H as written, with one addition found on the way in. The generated
`apk_data.rs` (from `build_support/papk.rs`) named `crate::sim_allocator` —
an alias that existed only because the RP `main.rs` happened to declare it.
A second family's simulator would have failed to compile on its first build
with an error pointing into `OUT_DIR`. The generator now spells the shared
path (`::picodroid_core::hal::sim::allocator::bypass()`), and the
"ESP crate has no capped allocator" branch beside it, which was already
dead, is gone. That is the porting seam's whole thesis in one line: an
obligation nobody wrote down, discovered by the build.

What moved: the ledger, `black_box` and all, `precharge`, `report`, the
charge/release pair (`picodroid-core/src/hal/sim/boot_budget.rs`, 3 new
unit tests); the sim `main` body and the closing banner (`sim_boot::main`);
the forwarding allocator (`declare_sim_global_allocator!`). What stayed:
the stack constants, `default_stack_bytes` and a `static MODEL` in the
family's `boot_budget.rs`. `BootLeaves`, `glue::run_sim`, the family's
`charge_task_spawn`/`release_task_spawn` wrappers and their sim/test cfg
fork are deleted; the family's `fs/mod.rs` is device-only, since the host
image is mounted by `sim_boot::main`. `register_sim_platform!` takes
`gc_roots`, `boot_budget`, `run_app` and emits `sim_main()` under the
invoking crate's `sim` feature. `platforms/rp/src`: −201 lines.

Verified: the boot banner reconciles exactly (`charged 71464 B of 71464 B
modeled`) on every smoke and on the capped `-l 200` run — the one number
that proves the model crossed the seam intact; `cargo test -p picodroid`
(48 pass, no cfg fork needed) and `-p picodroid-core` (261 pass); pre-commit
green. **Flash, rp2040 `--release`: byte-identical** (`.text` 697,776 /
`.rodata` 175,552 / `.data` 2,300), as it must be for a simulator-only
change.

### A3 — S3 landed: one USB identity (2026-09-03)

§3.E as written. `pdb_protocol::usb` owns VID 0x1209 / PID 0xCDC0 and the
two strings; `tools/pdb`'s port scan imports them instead of restating
them. `picodroid_core::pdb::usb_cdc` builds the device, configuration,
string and line-coding tables from those constants with `const fn` — the
string descriptors are encoded at compile time and a renamed string that no
longer fits its table fails the build rather than truncating on the wire.
The family's `hal/rp/pdb_usb/protocol.rs` and its `#[path]` test shim are
gone (its six `assemble_u32_le` tests with it — the function was
`u32::from_le_bytes`), and so is the simulator's empty `pdb_usb` stub: the
family's `pub use chip::pdb_usb` is device-only now, like its one consumer.
Ten descriptor tests run in core, one of them the new "identity agrees with
the protocol crate" check.

Verified: `cargo test` for `pdb-protocol` (31), `pdb` (16), `picodroid`
(34 — the 14 that moved are gone) and `picodroid-core` (274); helloworld
in the simulator; pre-commit green. **Flash, rp2040 `--release`:
byte-identical** — the same bytes, emitted from a different crate.
`pdb ping` on hardware is owed and batched into S6's bench session.

### A4 — S4 landed: honest names, live tests, one root (2026-09-03)

`hal/rp/i2c/protocol.rs` is `i2c/timing.rs` and `spi/protocol.rs` is
`spi/xfer.rs` (not the predecessor's `spi/regs.rs`: the file's load-bearing
content is the live `SpiXferState`, not registers). `pwm.rs` and `adc.rs`
are directories now, each with a `math.rs` that has no register in it and a
`#[path]` shim in `main.rs`, so their tests compile on the host for the
first time: 8 pwm and 3 adc, with the pwm ones taking the peripheral clock
as an argument and checked at both 125 and 150 MHz — the old ones assumed
125 MHz and would have failed on the default (rp2350) board had they ever
run. `build_support/config.rs` gained `repo_root` (walks up to the
directory holding `build_support/`) and `is_embedded`; both build scripts
call them instead of counting `.parent()`s differently.

Verified: `cargo test -p picodroid` 45 (34 + 11), `-p picodroid-core` 274;
helloworld in the simulator; pre-commit green. **Flash, rp2040 `--release`:
byte-identical** — a constant passed as an argument inlines to the same
code.

### A5 — S5 landed: the Java-array wrappers are trait defaults (2026-09-03)

§3.D as written. `hal::array_io` stages a Java `byte[]` through a 64-byte
stack buffer; `HalI2c::{write,read}` and `HalSpi::{transfer,write}` have
default bodies over the slice methods, so a family implements
`write_slice`/`read_slice` and `write_raw`/`transfer_raw` and gets the
`picodroid.pio` entry points for free. Four hand-written copies are gone on
the RP side, four on the simulator's, four delegations in `glue.rs`, four in
`test_platform.rs`. Nine helper tests cover the staging: exact bytes, only
`len` of a longer array, copy-back of what the bus produced and no more, the
empty read, the error path, and the cap — inclusive at 64, refused at 65,
clamped when a family claims more than the buffer.

Two things fell out. `SpiXferState` lost its 64-byte `staging` buffer and
`SpiOp::WriteOnly`: only the deleted Java `write` used the interrupt-driven
write-only path (slices go through DMA), so clippy on the device build
flagged the variant the moment its one constructor was gone. And the
simulator's Java `I2cDevice.read` now answers through `read_slice` — the
BME688 fake at 0x77 — instead of zeros, which is better parity, recorded
here so nobody hunts for why a sim read changed.

Verified: `cargo test -p picodroid` 44 (one staging test retired),
`-p picodroid-core` 283 (+9); helloworld, `i2cdemo` (its bus scan runs
through the default `write` → `write_slice`) and `spidemo` (its transfers
through `transfer_raw`'s loopback) in the simulator; pre-commit green.
**Flash, rp2040 `--release`: `.text` 696,104 (−1,672), `.rodata` 175,608
(+56), net −1,616 B.** E3's prediction held with room to spare; the
fallback is not needed.

### A6 — S6 landed: one event ring, one touch override (2026-09-03)

§3.B and §3.C as written, E2 included: the simulator's `hal/sim/gpio.rs`
holds `Mutex<GpioEventRing<64>>` — the device's ring at the device's size,
so a stalled simulator drops and warns exactly as a stalled device does
where the old unbounded `VecDeque` hid it. The device keeps its timestamp
(`now_us`) and its wake semaphore beside the ring; `inject` now runs under
`cortex_m::interrupt::free`, which closes a real race the hand-written ring
had (the PDB task and the ISR both wrote it, with nothing between them).
`TouchOverride` replaces the three atomics in `hal/rp/touch.rs` and the
three in `hal/sim/display.rs`; both readers `match` on `OverrideSample`.
Nine tests: ring order, capacity `N - 1` (a ring using all `N` slots fails
it), wrap-around, the drop tally, an empty ring; the override's state
machine, re-press after release, release without inject, full-range
coordinates through the packing.

Verified on the host: `cargo test -p picodroid` 44, `-p picodroid-core`
292; helloworld in the simulator; pre-commit green. **Flash, rp2040
`--release`: `.text` 696,204 (+100 over S5), `.rodata` 175,608 (0)** — the
acquire/release pairs and the interrupt mask, within the ±50–100 B
expected; cumulative −1,516 B against the baseline. **Owed and batched
into the bench session after S7:** scripted input through navdemo in the
simulator, and on hardware real buttons and touch, `pdb input
tap/swipe/keyevent`, idle-sleep wake, and S3's `pdb ping`. Recorded as A8
when run.

### A7 — S7 landed: the flash slot is an adapter, the geometry is one type (2026-09-03)

§3.F and §3.G as written. `install::slot` holds `PapkSlotFlash` (three
constants, `erase_range`, `program_range`, `reset`), `PapkSlot<F>` (the
`PapkFlash` impl: meta sector plus whole data sectors on erase, page bounds
and offsets on write, `build_meta_page` programmed last) and `read_mapped`
(the boot-time read of an installed image through a memory-mapped slot).
The family's `packagemanager/mod.rs` is a `RpFlash: PapkSlotFlash` over
`hal::flash`'s two primitives and `type RpPapkFlash = PapkSlot<RpFlash>`;
`flash.rs` lost its thirty-line PAPK layer and its two layout constants, and
`read_flash_papk` is one call. Its three `debug_assert!`s on the filesystem
region — which never ran on a device — are one `assert!` with a
`&'static str`. `FsGeometry` gained `DEFAULT` (a `const`, so the family
asserts its flash matches at compile time), `resolve` and `check_prog`;
both backing stores call them, and the RP store's `geometry()` override and
two constants are gone. Eight slot tests: erase rounding, page placement,
the last page fits and the next is refused without reaching flash, the
commit page, the constants, and `read_mapped` on an installed, an erased,
and an over-long image.

Verified on the host: `cargo test -p picodroid-core` 300 (+8),
`-p picodroid` 44; helloworld in the simulator; `bootcount` on a fresh
isolated image (`PICODROID_SIM_FS`) three runs, `Boot #1`, `#2`, `#3` —
the whole read/write path through the geometry helpers; pre-commit green.
**Flash, rp2040 `--release`: `.text` 696,280 (+76 over S6), `.rodata`
175,720 (+112)** — the assert message and the monomorphised adapter; more
than the ≈0 predicted, and still −1,328 B cumulative. Hardware: the bench
session that follows (A8).

### A8 — S8 landed: one hooks installer, one scanner, a seam guard (2026-09-03)

§3.I and §3.J as written. `picodroid_core::rtos::freertos::install_heap_atomic_hooks`
replaces the fourteen lines both boots carried (`boot_tasks.rs` and
`sim_boot.rs` each make one call); `rtos.rs` became `rtos/mod.rs` to hold
it. `test_support/source_scan.rs` owns the walker, the comment stripper and
the path helper; `task_affinity.rs` includes it and keeps only its rules,
and its "nothing creates a task outside `spawn`" test now scans this crate
alone. Core's new `rtos::seam_guard` scans `picodroid-core/src` — minus
`hal/sim/**`, `rtos/freertos.rs` and itself — for any FreeRTOS API name
(`vTaskDelay`, `xQueueSend`, `pvPortMalloc`, …), `freertos_rust`,
`Task::new()`, `.core_affinity(`, `set_core_affinity(` and `CurrentTask::`,
and pins that it saw `lifecycle.rs`, `threads.rs`, `main_queue.rs` and
`sim_boot.rs`. `gc_root_scan.rs` keeps its own eight-line walker: a nested
`#[path]` inside a `#[path]`-included file resolves against a directory
named after the module, which is the fallback §3.J allowed for.

**Both scans were sabotaged before this landed**, per the lesson B6/B8
recorded three times. A `vTaskDelay(1)` planted in `lifecycle.rs` failed
the seam guard with `lifecycle.rs: vTaskDelay`; a `"Task::new()"` planted
in `app.rs` failed the affinity scan with `platforms/rp/src/app.rs:
Task::new()`. (The first attempt at the second sabotage used a real
`freertos_rust::Task::new()`, which does not compile on the host and so
proved nothing — recorded so the next person plants a string literal.)

Verified: `cargo test -p picodroid-core` 302 (+2), `-p picodroid` 44;
helloworld in the simulator; pre-commit green. **Flash, rp2040
`--release`: `.text` 696,292 (+12 over S7), `.rodata` 175,720 (0)** — the
shared installer is a call where the block was inline; cumulative −1,316 B.

### A9 — the bench session for S3, S6 and S7 (2026-09-03)

**`testbench_rp2350`**, the only board attached; the rp2040 half of every
hardware gate below is owed, as B7/B9/B14 recorded for their stages. Firmware
at S7 (`f09b698`), debug profile with line numbers, flashed with
`scripts/flash.sh`.

| Check | Result |
|---|---|
| boot + RTT (helloworld) | passes |
| `pdb ping` (S3's descriptors) | `picodroid/2.1`, max PAPK 1020 KB, framework-map 0.0.0 — the host tool found the device by the identity it now imports from `pdb-protocol` |
| `pdb sysmon` | 11 tasks decode: pdb, IDLE0/1, flashpark, Tmr Svc, fs, 4× jvm-bg, jvm |
| `pdb install` ×3 (navdemo, helloworld, navdemo) | `Install complete.` each time — the `PapkSlot` erase/page/commit path on real flash |
| device-side `STATUS_INCOMPAT` (a `--shrink` PAPK against no-shrink firmware, `--skip-host-check --expect-rejected`) | refused with `framework-map-version mismatch`; `pdb ping` answered afterwards, so the board was still running and its flash intact |
| `pdb input tap 110 55`, `input swipe 200 120 40 120 200` | acknowledged (`STATUS_OK`) — the `TouchOverride` path through the bridge |
| `pdb input keyevent 4` | `INPUT returned ERR (no such key)` — correct on a board with no `[[button]]` |
| `bootcount` across a reflash | run 1 counted from `#1`; run 2, after a full reflash, resumed at `#101` — the value run 1 wrote survived, through `FsGeometry` and the unchanged flash primitives |

Two things the session could not show. The probe's RTT attach dies when
an install resets the board (the first `flash.sh` exited with
`Error: Exception` at that moment), so the navdemo log lines a tap should
produce on hardware were not captured; the bridge acknowledged the tap and
the same path was watched end to end in the simulator instead — see below.
And with no buttons on this board the idle-sleep wake and the real-button
edge path were not exercised; the ring is the same type the simulator
drove, and `inject` is the only device-side change to it.

**Simulator, scripted input (S6):** on `pico_enviro_mon`, `input keyevent
23` (the board's ENTER) pressed "Open Detail" and `input back` finished
it — `Home: launching Detail → Detail.onCreate → Detail.onDestroy →
Home.onRestart`, the full cycle through the shared ring. On
`testbench_rp2350`, `input tap 110 55` opened Detail through the shared
override; `input back` was refused ("no buttons on this board"), as it
should be.

**An observation that is not this doc's to fix.** `bootcount` re-runs its
`Application.onCreate` continuously on this board — about once a second on
this branch, and at the branch point `80aad79` on main about seven times a
second (216 `Boot #` lines in ~30 s, `#750` → `#965`). `helloworld` does
not. It is pre-existing, so it is recorded here rather than chased; the
likely shape is a notification left pending on the JVM task after
`run_app` returns (the fs worker notifies its waiter; `wake_all_parked`
notifies parked tasks), which makes the supervisor's "wait for the next
install" return at once. The persistence check above does not depend on
it: what run 2 read was what run 1 wrote.
