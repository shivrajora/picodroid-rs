# Design: extract the family-neutral residue left in `platforms/rp`

> Produced 2026-07-26/27 by a code-structure audit session (three parallel
> exploration agents over `pdb`/`packagemanager`/`fs`, the root files and
> `glue.rs`, and the `hal` tree plus `build.rs`; then a design pass). Every
> claim below was checked against source at `59970bc`. Amendments are appended
> at the bottom and OVERRIDE the design body where they conflict. Execute from
> this doc; update it if reality diverges.
>
> Direct successor to `docs/designs/shared-core-extraction.md`, whose
> vocabulary (HAL traits + facade, `Rtos`, `PlatformHooks`, registration
> macros, the shadow-twin rule) this doc assumes.

## 0. Why this exists

The shared-core extraction moved ~26K lines out of the binary crate and closed
with `platforms/rp/src` at ~9.7K, described in its §5 as the family-specific
end state: `main.rs`, `glue.rs`, `hal/rp/**`, `fs/**`, `pdb/**`,
`packagemanager/**`, the sim allocator, `boards/**`.

That description was a prediction, not a measurement. This audit measured it.
**Roughly 4K of those 9.7K lines are MCU-family-neutral** — a second family
would copy them verbatim, which is precisely the mechanism that gave the
removed ESP scaffold 17 drifting sim-stub twins (`shared-core-extraction.md`
A6). Three of them are *already* twins today:

1. **PDB wire constants.** `platforms/rp/src/pdb/protocol.rs` and
   `tools/pdb/src/protocol.rs:5-49` define the same `CMD_*`/`STATUS_*`/
   `INPUT_*`/`KEY_META_*`/`INSTALL_PEEK_BYTES` by hand. The host copy carries
   the comments "mirrors the device constant" (`:9`, `:36`, `:42`) — sync by
   convention, on the one surface where divergence means a failed or corrupt
   install.
2. **The simulator's RTOS backing.** `glue.rs:671-1021` (351 lines) is 82.5%
   normalized-identical to `picodroid-core/src/test_platform.rs:318-520`.
   `test_platform.rs:317-322` says so itself: *"This duplicates a little of
   the simulator's `std` backing in the platform crate. That resolves at stage
   8, when the sim HAL moves into this crate and both can share one
   implementation."* Stage 8 landed (A6); the dedup did not.
3. **Input injection.** `pdb/input.rs` and
   `picodroid-core/src/hal/sim/display.rs:601-655` implement the same
   `keyevent`/`tap`/`swipe` verbs with the same timing constants (40 ms key
   gap, 120 ms tap hold, 80 ms settle, `STEPS = 12`) and a byte-identical
   `lerp` (`input.rs:127-130` vs `display.rs:596-599`). `display.rs:554-556`
   documents the duplication as deliberate.

Plus four structural defects the audit surfaced:

- The **PAPK boot-meta flash layout** (magic `0x5044_4231`, flags, len,
  4096-byte meta sector) is written three times — `hal/rp/flash.rs:16-19`
  (hand-parsed at `:68-81`, hand-built at `:247-253`), `hal/sim/flash.rs:8`,
  and `build_support/papk.rs:441-450`, whose comment "Layout matches
  `read_flash_papk()` on-device" is a comment where a shared constant belongs.
  `docs/designs/papk-format-crate.md` already records this as its own
  follow-up.
- `Pull` / `EdgeTrigger` / `GpioEvent` are **defined three times**
  (`picodroid-core/src/hal/types.rs`, `picodroid-core/src/hal/sim/gpio.rs`,
  `platforms/rp/src/hal/rp/gpio.rs`), and in a sim build `glue.rs:100-114`
  converts between two of them that live in the *same crate*. `glue.rs:96-99`
  predicted its own deletion at stage 8.
- **798 lines of `pdb/` + `packagemanager/` have never been compiled on a
  host.** `main.rs:43-44` gates the tree out of `test` and `sim`, and
  `hal/sim/pdb_usb.rs` is four comment lines. The install state machine — the
  one code path that can brick a device — has zero tests.
- A **live parity divergence**: the device's `HalClock::sleep`
  (`hal/rp/system_clock.rs:2-7`) returns early when a debugger has requested a
  JVM stop; the simulator's does not. Nothing in the `HalClock` trait,
  `contract.rs`, or the porting guide says a family's `sleep` must do this, so
  it is an unwritten obligation as well as a `Thread.sleep`-interruptibility
  difference.

One loose end from the predecessor is also closed here: A2's deferred runtime
`provider_count()` assertion (`shared-core-extraction.md:528-537`) was to land
"with the flash delta measurement". The measurement landed in A7; the
assertion did not. `provider_count` appears today only in
`picodroid-core/src/gc_roots.rs:122` and its own unit tests.

**The acceptance test is unchanged** — `shared-core-extraction.md` §5's
"what a future `platforms/stm32` must provide". This work shrinks that list.

## 1. Scale

Measured over `platforms/rp/src` (9,494 LOC of Rust, plus 1,959 LOC of C and
headers under `hal/rp/port/`).

| Body | LOC | Where it is |
|---|---:|---|
| Simulator platform layer | ~1,750 | `sim_allocator.rs`, `sim_heap4.rs`, `glue.rs` sim arms, `hal/sim/*` stubs |
| Networking (FreeRTOS+TCP / cyw43) | ~820 | `hal/rp/net.rs`, `wifi_task.rs`, 3 neutral `.c` files, half of `cyw43_port.c` |
| PDB stack | ~750 | `pdb/{protocol,task,input,sysmon,cdc_transport}.rs`, `hal/rp/pdb_usb/protocol.rs`, `crc32.rs` |
| Install path | ~290 | `packagemanager/{transport,install}.rs` |
| Filesystem | ~410 | `fs/{mod,worker,storage_host,error}.rs` |
| Misc HAL | ~340 | GPIO event ring, touch override, `boot_budget` engine, `hal/mod.rs` boilerplate, JVM supervisor loop |

The genuinely silicon-bound residue — register drivers, `with_xip_disabled!`,
the USB DPRAM state machine, DMA, linker symbols, the pico-sdk C shims that
exist for the RP FreeRTOS SMP ports — is real and stays. §5 lists it.

## 2. Decisions

Confirmed with the maintainer before execution.

### D1 — PDB wire format becomes a crate: `pdb-protocol`

A new no_std, zero-dependency workspace crate, peer of `papk-format`, holding
the frame magic, every `CMD_*`/`STATUS_*`/`INPUT_*`/`KEY_META_*` constant,
`INSTALL_PEEK_BYTES`, `crc32_frame`, and the nibble-table `Crc32` moved
verbatim from `platforms/rp/src/crc32.rs` with its known-vector tests.

Rejected: a module inside `picodroid-core`, because `tools/pdb` is a host
`std` CLI and depending on core would drag the LVGL C build and `pico-jvm`
into it — so the CLI would keep its hand-written copy and the drift would
survive, defeating the point. Rejected: folding into `papk-format`, which is a
package *container* format with different consumers and a different change
cadence; a debug transport is not the same concern.

`Crc32` lands in `pdb-protocol` rather than in core because both firmware
consumers are PDBP concerns: frame CRC (`pdb/protocol.rs:45`) and install
stream verification (`install.rs:142`), whose CRC is seeded with the
`CMD_INSTALL` byte and is literally part of this wire protocol. `tools/pdb`
drops `crc32fast` — the crate's known-vector tests already pin IEEE
equivalence, which is the property the removed comment at `crc32.rs:9-10` was
asserting by hand.

This also fixes a layering inversion: `install.rs:15` currently reaches into
`crate::pdb::protocol` for `INSTALL_PEEK_BYTES`, which forces `pdb/mod.rs` to
keep `protocol` outside its own cfg gates. It is an install-protocol
parameter, and after the move both sides import it from the same neutral
place.

The PAPK **flash boot-meta** layout is a separate concern with a separate
home: `papk_format::flash_image` (D-note in §4 Stage 3), per that crate's own
recorded follow-up.

### D2 — LittleFS returns to `picodroid-core` behind an optional feature

`fs/mod.rs`'s mount → `Corrupt` → format → remount recovery, the `with_fs`
singleton, the cell modules, `storage_host.rs`, and glue's 95-line `HalFs`
impl move to `picodroid-core/src/fs/` under `feature = "littlefs"`, default
off. `littlefs-rust` returns as an **optional** dependency.

A7 recorded dropping that dependency as the `HalFs` seam "paying for itself
twice", so this needs justifying rather than assuming. The invariant A7
actually bought is *core is not forced to know LittleFS*, and an optional
default-off feature preserves it exactly: `cargo build -p picodroid-core`
still has zero LittleFS, and `HalFs` remains the only contract — a family with
FAT or NVS ignores the feature and implements `HalFs` directly. Against that,
leaving it costs every family ~410 lines of subtle correctness (recovery
ordering, the same-core XIP discipline, byte-compatible host images) *and*
keeps a device-vs-sim cfg split inside a family crate, which is the exact
drift-generating shape A6 eliminated for `hal/sim/`.

Mechanically, statics cannot be generic, so the device backing store crosses
at init: `fs::init_device(storage: impl FsBackingStore + Send + 'static)`
boxes into an internal `DynStorage`. That `dyn` is *inside* core, not on a
`__pd_*` registration seam — the no-dyn rule is about the seam and the
band-flush hot path, and this is one vtable hop per block operation against
millisecond-scale flash.

**Recorded fallback**, decided at Stage 5 start rather than now: if
`littlefs_rust::Filesystem<DynStorage>` fights the box, keep `fs/mod.rs`
family-side and land only the shared `serial_worker` and `storage_host`.

### D3 — Networking is deferred to its own design doc

~820 lines with zero RP content, and the largest single body in the audit —
but it spans the board schema, C build plumbing, and a new cfg channel, and it
pays nothing until a second *networking* family exists. It gets
`docs/designs/network-stack-extraction.md` when that day comes. The landing is
pre-decided so the analysis is not redone; see §6 (Phase N).

### D4 — The JVM supervisor loop is relayered, not moved

`hal/rp/boot.rs:140-176` — clear-stop → `run_jvm_with` → abort child delays →
drain `ACTIVE_JVM_THREADS` → `FLASH_PARK_REQUESTED`/`CORE0_PARKED` handshake →
`take_notification` — is framework lifecycle with no silicon in it, and
getting it wrong means PDB installs silently corrupt. It is tempting to hoist.

We do not, because that park/notify protocol exists for a specific reason:
this family executes from the same XIP flash the installer erases, on a core
that must be parked. Hoisting needs five or more new required hooks (clear
stop, abort child delays, drain active children, park request/ack, wait for
notification) that would encode *this family's flash topology* into
`PlatformHooks` on a sample size of one. A dual-bank or run-from-RAM family
has a structurally different loop.

What is genuinely shared and corruption-prone — install status semantics, the
stream-and-verify state machine, the `CoreCoordinator` contract — moves in
Stage 3. The loop itself relocates out of the HAL layer, where it does not
belong (it reaches into `crate::fs::worker`, `crate::pdb::pending` and
`crate::boot_budget`), into `platforms/rp/src/boot_tasks.rs`. The porting
guide gains a "JVM supervisor obligations" checklist with this loop as the
reference implementation, and because Stage 4 adds exactly the notification
primitives it uses, a future family writes it against the seam. **Revisit
trigger: second-family bring-up.**

### D5 — `Rtos` gains task notifications and pointer-width queues, nothing else

Six functions and one opaque type. Justified by four existing consumers, not
speculation: `fs/worker.rs` (queue of `usize` pointers plus notify/wait),
`pdb/pending.rs:52-70` (notify), `hal/rp/boot.rs:157,170,176` (wait), and
`wifi_task.rs:43,63` (current + wait) use these two FreeRTOS facilities and
nothing more. The existing `u32` `queue_send`/`queue_recv` stays untouched —
`main_queue` and `background_pool` deliberately pack words — so a parallel
`_ptr` triple avoids both truncating host pointers on 64-bit and disturbing
the word API.

### D6 — `register_sim_platform!` takes family policy as macro parameters

Core gains shared `SimRtos` and sim-hook bodies, but two leaves remain family
policy: GC-root registration, and the simulator's thread-spawn heap charge
(whose boot-budget table is chip-gated platform data and stays put). Rather
than adding `PlatformHooks` methods every *device* family must also stub —
`host.rs:9-14`'s objection to exporting the boot budget still stands — the
macro takes them as parameters and bakes them into the generated
registrations.

The leaf artifact still registers, `defmt`-style (§3.C of the predecessor),
and "an empty body is a decision rather than an unasked question" survives as
"an explicit macro argument is a decision". This also resolves the
`sim_charge_thread_spawn` history cleanly: the hook was dropped because only
platform code consumed it; now core's sim glue consumes it, and the macro *is*
that glue's registration point. **`PlatformHooks` is unchanged.**

### D7 — Sysmon splits at the encoder

The 20-byte header / 28-byte-per-task encoder, the snapshot ring, and
`compute_cpu_pct` (`pdb/sysmon.rs:116-229`) define the wire contract
`tools/pdb/src/sysmon.rs` parses, so they move to core and gain a golden-bytes
test — drift becomes a red test rather than a field report. The `TaskStatusC`
ABI mirror (`sysmon.rs:23-45`) stays platform-side behind a `SysmonSource`
trait: its doc comment records that it is pinned to *this* build's
`FreeRTOSConfig.h` (40 bytes with `configUSE_CORE_AFFINITY`, not 36), which no
other family can reuse a byte of.

### D8 — The PDB and install seams are generic parameters, not `__pd_*` symbols

The facade-and-macro machinery exists to serve ~90 scattered framework call
sites and framework statics. The PDB stack has exactly one entry point (the
platform's task spawn), no shared statics needing platform types, and it is
the porting guide's *optional* item 5. So it crosses on generic trait
parameters at that single call site: zero new extern surface, no cfg-gating of
link symbols for boards without a transport, monomorphization in the platform
crate (one caller — no `bg_worker`-style duplicate instantiation), and
mock-based host tests for free. This is how `InstallTransport` and
`CoreCoordinator` already work (`packagemanager/transport.rs:33`,
`install.rs:31-43`); the seam merely moves to the right side of the crate
boundary.

## 3. Seams

### 3.A `pdb-protocol` crate

`pdb-protocol/src/lib.rs`, `#![no_std]`, no dependencies:

```rust
pub const FRAME_MAGIC: &[u8; 4] = b"PDBP";
pub const CMD_PING: u8 = 0x00;
pub const CMD_INSTALL: u8 = 0x01;
pub const CMD_SYSMON: u8 = 0x02;
pub const CMD_INPUT: u8 = 0x03;
pub const INPUT_KEY: u8 = 0x01;
pub const INPUT_TAP: u8 = 0x02;
pub const INPUT_SWIPE: u8 = 0x03;
pub const KEY_META_DOWN_UP: u8 = 0;
pub const KEY_META_DOWN: u8 = 1;
pub const KEY_META_UP: u8 = 2;
pub const STATUS_OK: u8 = 0x00;
pub const STATUS_READY: u8 = 0x01;
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;
pub const STATUS_INCOMPAT: u8 = 0xFC;
pub const INSTALL_PEEK_BYTES: usize = 512;

pub mod crc32;                    // Crc32 { new, update, finalize }
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32;
```

Doc comments carry the frame layouts, which are today spread across both
copies: request `[PDBP][cmd][len:4 LE][payload][crc:4 LE]`, response
`[PDBP][status][len:4 LE][payload]`, the 9-byte install Phase-A header, and
the `CMD_INPUT` payload shapes.

### 3.B `PdbTransport` — `picodroid-core/src/pdb/`

Generic parameter, no externs (D8):

```rust
pub trait PdbTransport {
    fn init(&mut self);
    fn read_byte(&mut self) -> u8;                 // blocking
    fn read_byte_timeout(&mut self) -> Option<u8>; // install streaming
    fn write_bytes(&mut self, data: &[u8]);
    fn drain_tx(&mut self);
    fn read_u32_le(&mut self) -> u32 { /* default: 4 × read_byte */ }
}

pub fn run_pdb_task(
    transport: impl PdbTransport,
    coordinator: impl crate::install::CoreCoordinator,
    sysmon: impl sysmon::SysmonSource,
    flash: impl crate::install::PapkFlash,
) -> !;
```

This is exactly the six-symbol surface `hal/contract.rs:44-49` asserts today
and the porting guide specifies, so the trait subsumes those assertions. The
chip-conditional busy-wait (`pdb_usb/mod.rs:501`, currently `cfg`-selected at
the *call site*, `cdc_transport.rs:48-51`) moves inside the platform's
`read_byte_timeout` where it belongs — the caller should not know why a read
times out differently on one chip.

### 3.C `PapkFlash` — `picodroid-core/src/install/`

```rust
pub trait PapkFlash {
    fn max_data_size(&self) -> usize;
    /// # Safety
    /// The JVM core must be parked (the `CoreCoordinator` contract) before
    /// erase or write: this family executes from the flash being erased.
    unsafe fn erase_region(&mut self, papk_len: usize);
    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool;
    unsafe fn commit_metadata(&mut self, len: u32);
    fn trigger_reset(&mut self) -> !;
}
```

Replaces the five direct `super::flash::*` free-function calls in
`install.rs` (`:65`, `:113`, `:127`, `:131`, `:166`) — the last
free-function seam in the install path, and the reason that path cannot be
tested on a host today.

### 3.D `SysmonSource` — `picodroid-core/src/pdb/sysmon.rs`

```rust
pub const MAX_TASKS: usize = 12;

#[derive(Clone, Copy, Default)]
pub struct TaskSample {
    pub name: [u8; 16],
    pub state: u8,
    pub current_priority: u8,
    pub base_priority: u8,
    pub stack_high_water: u16,
    pub task_number: u16,
    pub run_time: u32,
}

#[derive(Clone, Copy, Default)]
pub struct SysmonSample {
    pub uptime_ticks: u32,
    pub free_heap: u32,
    pub min_free_heap: u32,
    pub total_run_time: u32,
    pub task_count: u8,
    pub tasks: [TaskSample; MAX_TASKS],
}

pub trait SysmonSource {
    fn sample(&mut self, out: &mut SysmonSample) -> bool;
}
```

Core owns the ring, `compute_cpu_pct`, the encoder, the `mem-diag` tail, and
the golden-bytes layout test.

### 3.E `Rtos` additions — `picodroid-core/src/rtos.rs`

Style-matched to the existing trait (opaque `Raw*` handles, `Timeout`):

```rust
pub type RawTask = usize;                      // 0 = no task context

fn task_current() -> RawTask;
fn task_notify(t: RawTask);                    // increment-style
fn task_wait_notification(t: Timeout) -> bool; // true = notified; clears on take
fn queue_create_ptr(depth: usize) -> RawQueue;
fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool;
fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize>;
```

Facade functions, `__pd_rtos_*` externs and `set_rtos!` arms mirror the
existing fifteen. Device backing: FreeRTOS direct-to-task notifications and
`Queue<usize>`. Simulator and test backing: a leaked per-thread
`(Mutex<u32>, Condvar)` notification slot and a `VecDeque<usize>`.

### 3.F `register_sim_platform!` — `picodroid-core`

```rust
picodroid_core::register_sim_platform! {
    gc_roots = crate::gc_root_registration::register_all,
    charge_jvm_child_spawn = crate::boot_budget::charge_thread_spawn,
}
```

Expands to `set_hal!` of the shared simulator HAL (plus `set_hal_net!` under
`cfg(has_network)`), `set_rtos!` of a generated type delegating to
`hal::sim::rtos` with `$charge_jvm_child_spawn` baked into the `JvmChild`
refusal arm, and `set_platform_hooks!` of a generated hooks type
(`stop_requested` → `false`; heap bypass, checkpoint and native stats →
`hal::sim::allocator`; `register_gc_roots` → `$gc_roots()`). The platform
wraps the invocation in `#[cfg(any(test, feature = "sim"))]`.

### 3.G `serial_worker` — `picodroid-core/src/executors/`

`fs/worker.rs`'s machinery, generalized: type-erased closure plus a `Work`
trampoline on the caller's stack, pointer through a queue, caller blocks on a
task notification. Built on §3.E. Public surface `spawn(name: &'static str)`
and `submit<F: FnOnce() -> R, R>(f: F) -> R`. `TaskKind` gains `FsWorker`;
the platform's `Rtos::spawn` maps it to `FS_STACK_WORDS`, core affinity and
priority — policy stays platform-side per the predecessor's §3.B.

### 3.H `fs` — `picodroid-core/src/fs/`, `feature = "littlefs"`

```rust
pub trait FsBackingStore: littlefs_rust::Storage {
    fn block_count(&self) -> u32;
    fn geometry(&self) -> FsGeometry { FsGeometry { block: 4096, prog: 256, read: 16 } }
}

pub fn init_device(storage: impl FsBackingStore + Send + 'static) -> Result<(), FsError>;
pub fn init_host_image() -> Result<(), FsError>;
pub fn with_fs<R>(f: impl FnOnce(&mut Filesystem) -> R) -> Option<R>;
pub struct LittleFsHal;   // impl HalFs
pub fn spawn_worker();
```

### 3.I `GpioEventRing` — `picodroid-core/src/hal/event_ring.rs`

```rust
pub struct GpioEventRing<const N: usize> { /* … */ }

impl<const N: usize> GpioEventRing<N> {
    pub const fn new() -> Self;
    pub fn enqueue(&self, pin: u8, rising: bool, t_us: u32) -> bool; // false = dropped, tallied
    pub fn drain(&self) -> Option<GpioEvent>;
    pub fn take_drop_report(&self) -> Option<u32>;
    pub fn has_pending(&self) -> bool;
}
```

Single-writer (ISR) / single-reader (task) contract documented on the type.
Timestamping and wake signalling stay caller-side, which is what keeps the
ring free of both the RP timer and the semaphore type. The simulator keeps its
`Mutex<VecDeque>`: cross-thread injection is a genuinely different concurrency
contract, and the doc records that rather than forcing one shape.

## 4. Stages

Every stage ends green: `./scripts/pre-commit` printing
`==> All checks passed.`, plus `./scripts/sim.sh --app helloworld`,
`--app benchmark`, `--app gcstress`, and
`perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky`. Every stage
that touches device-compiled code records the rp2040 release `.text`/`.rodata`
delta; cumulative budget ≤ ~2 KB, as before.

| # | Stage | Scope | Platform LOC |
|---|---|---|---:|
| 0 | Design doc | This file. | — |
| 1 | `pdb-protocol` crate | New crate; `crc32.rs` and `pdb/protocol.rs` move into it; five device consumers and `tools/pdb` re-import; `crc32fast` dropped from the CLI. | −151 |
| 2 | Sim platform consolidation | `sim_heap4.rs` + `sim_allocator.rs` → core; `SimRtos` → core; `register_sim_platform!`; delete `hal/sim/` stubs; dedup `test_platform.rs`; close the A2 gap. | −1,750 |
| 3 | PDB stack + install | `picodroid-core/src/pdb/` and `src/install/` behind §3.B–§3.D; `papk_format::flash_image`; supervisor relayering (D4); sleep-parity fix; input-inject dedup. | −620 |
| 4 | `Rtos` additions | §3.E: trait, facade, externs, macro arms, three backings. | +60 |
| 5 | Filesystem | §3.G + §3.H behind `feature = "littlefs"`. | −390 |
| 6 | Misc HAL sweep | Event ring, touch override, type unification, `boot_budget` split, dead code, renames, `declare_family_hal!`, `build_support` tidy. | −250 |
| 7 | Guards, docs, measurement | Close-out, A7-style. | — |
| N | Networking | Separate doc; see §6. | (−820) |

Stages 1 and 2 are the first execution session; 3–7 follow from this doc.
Each is independently green and stoppable — stopping after 1 already kills the
worst drift, and 1–2 removes ~1,900 lines with no device-code motion.

### Stage 1 — `pdb-protocol`

Create `pdb-protocol/{Cargo.toml,src/lib.rs,src/crc32.rs}` and add it to the
workspace `members`. `git mv platforms/rp/src/crc32.rs` into it (tests ride
along, becoming in-crate); delete the platform module declaration. Delete
`platforms/rp/src/pdb/protocol.rs` and re-import `pdb_protocol::…` in the five
consumers (`pdb/task.rs`, `pdb/cdc_transport.rs`, `pdb/input.rs`,
`pdb/sysmon.rs`, `packagemanager/install.rs`). In `tools/pdb/src/protocol.rs`
replace `:5-49` with `pub use pdb_protocol::*;`, keeping the `std` I/O helpers
(`send_frame`, `send_install_header`, `send_install_data`, `recv_response`,
`status_str`, the poll constants) and re-targeting the chunked-CRC test.

Risk is low: behaviour is byte-identical by the known-vector tests, and the
one silent hazard — device and host CRCs diverging — becomes impossible by
construction. Flash delta expected ≈ 0; measure anyway, since `crc32` now
codegens in a different unit.

### Stage 2 — Simulator platform consolidation

Biggest single stage by LOC, and the lowest device risk: nothing here compiles
into firmware.

`git mv sim_heap4.rs → picodroid-core/src/hal/sim/heap4.rs` (verbatim; it has
no imports at all). `git mv sim_allocator.rs → .../hal/sim/allocator.rs`,
exporting `pub static SIM_ALLOCATOR: CappedAllocator` (its `new()` is already
`const fn`) plus the module API; its `include!(OUT_DIR/heap_config.rs)`
re-targets core's OUT_DIR, which already emits that artifact. Core's
`Cargo.toml` gains host-only `libc`, mirroring what `minifb` did in A6; the
platform's own host-only `libc` then has no consumer and is dropped.

`#[global_allocator]` is legal only in the binary crate, so `main.rs` keeps a
~12-line newtype forwarding `GlobalAlloc` to core's static — written out
explicitly rather than relying on a blanket impl. The allocator's routing
tests move in-crate to core against a locally constructed `CappedAllocator`,
which needs no global registration.

`SimRtos` is `glue.rs:671-1021` moved to `hal/sim/rtos.rs`, with its six
`crate::sim_allocator::bypass()` calls becoming `super::allocator::bypass()` —
**sibling calls, not seam round-trips**, which is A6's lesson restated. The
`PICODROID_PARITY_STRICT` panic moves with it: refusing to fake
`Thread.start` is shared simulator policy, not family policy.

Then `register_sim_platform!` (§3.F), and `glue.rs` deletes its sim RTOS arm,
its sim hook branches, and the redundant local `heap_config.rs` include (the
device arm reads `picodroid_core::board_cfg::heap::DEVICE_HEAP_BYTES`, which
core already publishes). `platforms/rp/src/hal/sim/` is deleted entirely —
`boot.rs` is one empty function, `flash.rs` two constants, `pdb_usb.rs`
comment-only — and drops off the pre-commit twin allowlist.

Finally, two debts: `test_platform.rs`'s `TestRtos` delegates its queue,
mutex and semaphore to the shared primitives (keeping its deliberate
refuse-all-spawn and inert tick), which completes the promise at `:317-322`;
and the platform's `register_all` asserts
`gc_roots::provider_count() == core::EXPECTED_PROVIDERS + EXPECTED_PROVIDERS`,
closing A2's residual blind spot — a source scan sees text, so a registration
compiled out by a `cfg` reads as present, whereas this checks what actually
registered.

Verification adds one `PICODROID_HEAP_LIMIT_KB`-capped simulator run: the
allocator's arming order and bypass coverage are what that cap exercises, and
a regression there is invisible to the ordinary smokes.

### Stage 3 — PDB stack and install path

The highest-consequence stage: install regressions corrupt devices silently.
Three mitigations. The mock-based host tests land **first**, in the same
stage, pinning current behaviour before the code moves — phase-A rejects
(empty, too large, incompatible) happening *before* any erase, peek replay,
CRC failure, park-timeout release ordering. The status and CRC bytes are
already single-sourced by Stage 1. And a hardware check is mandatory before
the stage closes: `pdb install`, `pdb sysmon`, and `pdb input
tap/swipe/keyevent` against a real rp2040 *and* rp2350.

Scope: `picodroid-core/src/pdb/{mod,framing,input,sysmon}.rs` and
`src/install/{transport,install}.rs` per §3.B–§3.D; `papk_format::flash_image`
(`MAGIC`, `META_SIZE`, `build_meta_page`, `parse_meta`) consumed by both
`hal/rp/flash.rs` and `build_support/papk.rs`, killing the comment-synced
triplication. The platform keeps `pdb/pending.rs` and gains a ~60-line
`pdb/platform.rs` holding the four impls.

Two fixes ride along. The supervisor loop relayers to `boot_tasks.rs` (D4).
And the sleep-parity divergence closes the right way round: shared code gains
the stop check (`os/system_clock.rs` early-returns on
`host::stop_requested()`), and the device's hand-rolled early return at
`hal/rp/system_clock.rs:2-5` is deleted — so the simulator gains
stop-responsiveness for free and no future family has to know the obligation
existed. Audit the calibration loops for the same need.

The input-inject dedup lands here too: a core helper module owning the press/
release gap, tap hold and settle, and the 12-step swipe lerp, consumed by both
`pdb::input` and the simulator's control channel.

### Stage 4 — `Rtos` additions

§3.E, plus the three backings and an extension of the `rtos_facade_links`
test. Nothing consumes them yet, so the risk is confined to the device gaining
~7 unused shims — tens of bytes, spent by Stage 5.

### Stage 5 — Filesystem

§3.G and §3.H behind `feature = "littlefs"`. The platform keeps
`fs/storage.rs` (flash geometry and the `__fs_start`/`__fs_end` linker
symbols — 74 lines is the right size for per-family residue) plus a ~25-line
init. Glue's `HalFs` impl is replaced by
`set_hal_fs!(picodroid_core::fs::LittleFsHal)`.

Verification adds a simulator persistence smoke — write a file, re-run, assert
it survives, reusing the `PICODROID_SIM_FS_KB` image — and a hardware File-I/O
check. D2's fallback is decided at the start of this stage, not retrofitted.

### Stage 6 — Misc HAL sweep

The event ring (§3.I) and the shared `hal::touch_override` module both touch
the **device input path**, so this stage repeats the hardware button/touch and
`pdb input` checks. Also: type unification (the sim modules `pub use
crate::hal::types`, the RP module adopts them, glue's converters and `lift`
delete, and `contract.rs:26-27`'s note about what the converters were covering
is updated); the `boot_budget` split at `:56`, keeping constants and
`BOOT_TASKS` platform-side while the accounting engine — including the
`black_box` subtlety a re-deriving family would get wrong — moves; deletion of
the dead `I2cOp`/`I2cXferState` and their tests; renaming the two misnamed
`protocol.rs` files (`i2c/timing.rs`, `spi/regs.rs`) since they hold silicon
constants, leaving the name to `pdb_usb`'s, which is genuinely wire format;
`declare_family_hal!` for `hal/mod.rs:39-96`; and `repo_root()`/`is_embedded()`
into `build_support/config.rs`.

Explicitly deferred, and recorded so a third family can reopen it: `pwm.rs`'s
`duty_to_cc`, `spi_bus`'s chunking loops, `output_pin`'s optional-pin
handling. Neutral but ~40 lines total; a seam would cost more than it saves at
n = 2.

### Stage 7 — Guards, docs, measurement

Per §7 below, plus the final baseline-to-end flash table appended as an
amendment, A7-style.

## 5. End state — what stays in `platforms/rp`

- `main.rs` — entry, exception handlers, FreeRTOS hooks, both
  `#[global_allocator]` statics (the attribute is binary-crate-only).
- `glue.rs` — the family's HAL/RTOS/hooks impls, three device registrations,
  and one `register_sim_platform!` line.
- `boot_tasks.rs` and `hal/rp/boot.rs` — task topology, the JVM supervisor
  loop (D4), clock init, the RP2350 `IMAGE_DEF` block.
- `pdb/pending.rs` — the single-core `unsafe impl Sync` justifications,
  `cortex_m::asm::sev()`, the FreeRTOS task-handle registry.
- `pdb/platform.rs` — transport, coordinator, sysmon source and PAPK flash
  impls; transport primitives and chip forks are family-owned.
- `hal/rp/**` — the silicon: the USB DPRAM control-transfer state machine,
  `with_xip_disabled!` and the RAM-resident erase/program functions, DMA, the
  DesignWare and PL022 timing constants, and the pico-sdk C shims that exist
  because the FreeRTOS RP SMP ports include them.
- `packagemanager/flash/mod.rs` — the `.papk_flash_init` linker section
  probe-rs writes.
- `fs/storage.rs` — flash geometry and the `__fs_start`/`__fs_end` symbols.
- `boot_budget.rs` constants and `BOOT_TASKS` — chip-gated stack policy is
  this family's memory model.
- `gc_root_registration.rs`, `hal/mod.rs`, `hal/contract.rs` (boot and flash
  assertions only), `boards/**`, `mcus/**`, and a `build.rs` of C builds,
  `memory.x`, pin configs and APK embedding.

## 6. Phase N — networking (deferred, D3)

Recorded so the analysis is not redone. `hal/rp/net.rs` (302 lines of
`extern "C"` FreeRTOS+TCP bindings), `wifi_task.rs` (68), and the neutral C —
`NetworkInterface_CYW43.c` (166), `net_init.c` (132), `libc_str.c` (43), and
~90 of `cyw43_port.c`'s 193 — contain no RP content whatsoever.

Four decisions, pre-made:

1. **C-build ownership stays family-side.** The LVGL precedent (§3.H of the
   predecessor) does *not* transfer: LVGL's C had no family headers, whereas
   every FreeRTOS+TCP translation unit compiles against the family's
   `FreeRTOSConfig.h` and port headers (`build_support/network.rs:41,124`).
   What moves is the *source*: the neutral `.c` files relocate to
   `platforms/shared/net-freertos-tcp/`, the same include-don't-crate pattern
   as `build_support/` and `test_support/`. `build_support/network.rs` gains a
   `neutral_dir` parameter, and the gate re-keys on the board's
   `network_type == "cyw43"` rather than `mcu_family == "rp"`
   (`platforms/rp/build.rs:79`) — nothing about either library is RP-specific,
   and that gate is what strands the code today.
2. **One entropy seam.** `net_init.c:76-88` is the single RP line: an LCG
   seeded from the timer at `0x400B000C`. It becomes
   `extern uint32_t picodroid_port_entropy32(void);`, provided by a family C
   file. (The comment there already admits it is not the TRNG it claims.)
3. **`net.rs` → `picodroid-core/src/hal/freertos_tcp.rs`** behind a new
   `network_freertos_tcp` cfg emitted by `build_support/board_cfg.rs` from a
   board property; symbols resolve at final-binary link, the same link-time
   back-edge class as the `__pd_*` shims, and it is never compiled in core's
   own tests since `net` is already gated on `all(not(test), has_network)`.
4. **`wifi_task` → core** over `Rtos::spawn` and §3.E's
   `task_wait_notification`, with `cyw43::set_poll_task` taking a `RawTask`.

The porting guide has no networking section at all today, despite ~820 lines
being required for a network-capable board. That gap closes when this phase
does; until then Stage 7 notes it explicitly.

## 7. Documentation and guards

- **`scripts/pre-commit`**: Stage 2 drops `hal/sim/mod.rs` from `TWIN_ALLOW`
  (the file is deleted). The BLD-02 expected count stays 0 — nothing in this
  plan introduces a `not(feature = "family-rp")` gate. No new guard scripts:
  every new invariant lands as a workspace test (`pdb-protocol` known vectors,
  the sysmon golden bytes, the install mocks, the runtime provider count).
- **Porting guide** (`website/src/content/docs/reference/porting-guide.md`):
  rewrite the `pdb_usb.rs` and `boot.rs` module sections around the four
  traits; add the "JVM supervisor obligations" checklist (D4), including *do
  not put a stop check in your HAL `sleep`* — shared code owns that after
  Stage 3; update the six-item "what a new port must provide" list (optional
  item 5 becomes "implement the four PDB/install traits"); the filesystem
  section becomes "enable core's `littlefs` feature and provide
  `FsBackingStore`, or implement `HalFs` yourself"; note the networking gap
  and point at Phase N. Delete the instruction to retype
  `PAPK_FLASH_MAGIC = 0x5044_4231` — asking a porter to hand-copy a wire
  constant is the defect this doc exists to remove.
- **`ARCHITECTURE.md`**: refresh the module map; add two boundary rules —
  *wire formats live in tiny protocol crates (`pdb-protocol`, `papk-format`),
  never hand-mirrored*, and *simulator policy leaves cross as
  `register_sim_platform!` parameters, not as `PlatformHooks` methods*.
- **`docs/parity-audit.md`**: refresh the stale paths (`hal/boot_budget.rs` at
  `:243`; TCH-01/DSP-01's `hal/sim/*` references, which now live in
  `picodroid-core`), and record the sleep-stop parity fix against its row.
- **`docs/designs/shared-core-extraction.md`**: one closing amendment pointing
  here.
- **`docs/designs/papk-format-crate.md`**: mark follow-up §(c) step 7
  (`flash_image`) executed by Stage 3.
- **`picodroid-core/Cargo.toml`** header: record the `littlefs` feature and
  the host-only `libc`.

## AMENDMENTS

*(append here as execution diverges; amendments override the body above)*

### B1 — `CRC_TAG_INSTALL` was a third copy (Stage 1)

§2 D1 names two copies of the wire constants. There was a third, and it was
the load-bearing one: `install.rs`'s `CRC_TAG_INSTALL = 0x01`, defined locally
so the install path could read transport-agnostically. The host seeds its
hasher with `CMD_INSTALL`, so the two are not merely equal by convention —
a divergence fails *every* install with `STATUS_CRC_FAIL`. It is now an alias
of `pdb_protocol::CMD_INSTALL`, which keeps the local name and single-sources
the value.

The host's four CRC tests (`crc_single_shot_matches_chunked` and friends)
moved into the crate rather than being deleted: they were asserting that two
independent implementations of one standard agree, which is exactly the
property that stops being a question once both ends compile the same code.
What remains host-side is the `std` framing, which is genuinely the CLI's.

**Flash, rp2040 `--release`:** `.text` −272, `.rodata` +64 (the 64-byte nibble
table now emits in its own codegen unit). Net −208.

### B2 — the sim glue is registered, not the sim HAL (Stage 2)

§3.F specifies `register_sim_platform!` as expanding to `set_hal!` plus
`set_rtos!` plus `set_platform_hooks!`. The `set_hal!` part is wrong and was
dropped: a family's HAL impls are **not** cfg-split. They delegate to its own
`hal` module, whose `mod chip` already selects the shared simulator, so one
set of impls serves both arms and the platform's existing `set_hal!` covers
the simulator too. Registering the HAL again from the macro would be a
duplicate-symbol collision.

So the macro covers exactly the two registrations that *were* duplicated:
`Rtos` and `PlatformHooks`. That is a better statement of the boundary anyway
— the HAL was never the thing families were copying.

`glue.rs` goes from 1,138 lines to 769, and `platforms/rp/src` from 9,494 to
7,852.

### B3 — the A2 assertion needs `assert!`, not `debug_assert!` (Stage 2)

A2 (of the predecessor) asks for a runtime `provider_count()` check and notes
it "costs a little RP2040 flash for the message". The obvious spelling is
`debug_assert!`, which would cost nothing — and would also never run: device
builds pass `--config profile.dev.debug-assertions=false` (`scripts/lib.sh`)
to buy back ~37 KB of flash, so the configuration the check exists to guard is
precisely the one it would be compiled out of.

It is therefore a real `assert!` with a `&'static str` message rather than
`assert_eq!`, whose two operands would drag in formatting machinery for a
number nobody reads. **Cost: +200 bytes** (`.text` +32, `.rodata` +168) —
which is what A2 predicted, and why it asked for the measurement.

### B4 — `CappedAllocator` needed a `Default` (Stage 2)

Caught by clippy on the way in, and worth recording as a class rather than a
line: `new_without_default` did not fire while the allocator lived in a binary
crate, because the type was not reachable public API. Moving a type into a
library subjects it to lints the binary never applied. Nothing else in this
stage tripped one, but the next mover should expect it.

### B5 — Stage 1 and 2 measured cumulative (Stages 1–2)

| Section | Baseline (`59970bc`) | After stage 2 | Delta |
|---|---:|---:|---:|
| `.text` | 703,736 | 703,496 | **−240** |
| `.rodata` | 195,728 | 195,960 | **+232** |

Net **−8 bytes** against a ≤ ~2 KB budget, and +200 of the `.rodata` is the
A2 assertion — i.e. the moves themselves are slightly net-negative and the
only real spend was a deliberate one.

Verification run for both stages: `./scripts/pre-commit` green, the four sim
smokes (helloworld, benchmark, gcstress, blinky), and for stage 2 an
explicit `-l 200` capped run, since the allocator's arming order and bypass
coverage are what a cap exercises and the ordinary smokes would not notice a
regression there. The twin guard's shortened allowlist was verified by
planting a stale `hal/sim/mod.rs` and confirming the guard rejects it.

### B6 — stage 3 split into 3a/3b/3c; the rest still open (Stage 3)

§4 anticipated stage 3 might split. It did, into commit-sized pieces that
each end green, of which three have landed:

| | Scope | rp2040 `.text` |
|---|---|---:|
| 3a | `papk_format::flash_image` | +80 |
| 3b | sleep-parity fix | +468 |
| 3c | install path → core behind `PapkFlash`, with tests | +12 |

**Still open: 3d** (the PDB stack — `PdbTransport`, `SysmonSource`, the
golden-bytes encoder test), **3e** (input-inject dedup), **3f** (supervisor
relayering into `boot_tasks.rs`). §3.B and §3.D stand as written.

**3b cost more than it looks.** Moving the stop check from the RP HAL to the
shared `SystemClock.sleep` native turns an inlined read of a crate-local
static into a real call across the host seam. +468 bytes is the price of the
divergence being fixed rather than duplicated, and it is the largest single
item in this work so far.

**3c's tests found their own gap.** The first sabotage — hoisting the erase
above the compat check, i.e. the bug that destroys a working install — was
*not* caught. Every test PAPK was compat-clean, so nothing exercised the one
rejection that happens after the park and before the erase. Two tests built on
real PAPKs from `papk-format`'s writer close it. Recorded because the lesson
generalises: a mock that always takes the happy path through a gate leaves
that gate untested, and the sabotage is what reveals it.

A second, subtler version of the same: the tests initially hardcoded the
`"0.0.0"` sentinel and so passed under `cargo test` and failed under
`scripts/test.sh`, which runs both shrink modes. They now derive from
`FRAMEWORK_MAP_VERSION`.

`FRAMEWORK_MAP_VERSION` itself moved out of `boot` into `framework_map`: it is
a build artifact with no JVM or graphics dependency and was only behind
`cfg(not(test))` because its host module is.

### B7 — measurement and hardware, stages 1–3c

| Section | Baseline (`59970bc`) | After 3c | Delta |
|---|---:|---:|---:|
| `.text` | 703,736 | 704,056 | **+320** |
| `.rodata` | 195,728 | 195,968 | **+240** |

**+560 bytes** against the ≤ ~2 KB budget, of which +468 is 3b's seam crossing
and +200 the A2 assertion — i.e. the two deliberate correctness purchases
exceed the total, and the moves themselves remain net-negative.
`platforms/rp/src` is at 7,614 lines, from 9,494.

**HIL, `testbench_rp2350`** (an rp2040 was not attached; the rp2040 half of
§4's stage-3 gate is still owed). Flashed and booted after 3a and again after
3c — booting at all exercises the boot-meta path on both the build script's
side and `read_flash_papk`'s. Then, per stage: four consecutive `pdb install`
runs clean after 3a, three after 3c; `sysmon` and `input` responsive;
device-side `STATUS_INCOMPAT` rejection returning with flash intact and the
device still running its previous app — the hardware counterpart of the test
3c added. Board left on a known-good helloworld.

One install failed to re-enumerate over USB after its post-install reset. It
followed a `kill -9` of an attached `probe-rs` RTT session, did not reproduce
in seven subsequent attempts, and a power cycle cleared it — the leftover
probe-claim failure the flash tooling already warns about. Recorded rather
than omitted because "it did not reproduce" is a weaker claim than "it never
happened", and the next person seeing it should know where to look first.

### B8 — stage 3 closed: 3d, 3e, 3f (Stage 3)

| | Scope | rp2040 `.text` |
|---|---|---:|
| 3d | PDB stack → core behind `PdbTransport` / `SysmonSource` | +380 |
| 3e | one implementation of synthetic input | ~0 |
| 3f | supervisor loop out of the HAL into `boot_tasks.rs` | −4 |

**§3.B held; §3.D changed shape.** The transport trait is as specified. The
sysmon split is not: the design has the platform fill a `SysmonSample` and
core encode it, which is what landed, but it also predicted the encoder would
be the interesting half. The interesting half turned out to be *where the
previous sample lives*. It belongs to the protocol — the host asks "since
when?" and the answer is "your last query" — not to whatever produced the
numbers, so `PREV` sits in core beside the encoder rather than in the source.

**The golden-bytes test needed a second pass, for the same reason 3c's did.**
Swapping `current_priority` and `base_priority` — adjacent bytes, the classic
silent layout drift — was not caught, because the fixture gave both fields the
value 15. This is the *third* instance of one failure mode in this work: a
fixture that cannot distinguish two things is not testing that they are
distinct. Distinct values per field, and both that swap and a shifted header
count byte now fail.

**Four copies of input injection, not two.** §4's stage 3e names the PDB
handler and the simulator's `input …` verb. It missed two more in the same
file: the older by-name `press|tap <button>` verb had its own 40 ms
press/release, and `touch up` its own 80 ms settle. `keycode_to_pin` had three
copies, and — this is the part worth keeping — two of them *had* to exist,
because the graphics event layer is `cfg(not(test))` and the simulator
front-end is not, so neither could call the other's. It moved beside the
generated `BUTTONS` table, which is always compiled. When a shared thing has
copies on both sides of a cfg, the fix is usually to move it under the cfg
rather than to pick a side.

**D4 survived contact.** The supervisor loop moved out of `hal/rp/boot.rs`
— where it never belonged, reaching into `fs::worker`, `pdb::pending` and
`boot_budget` — into `boot_tasks.rs`, and stayed family-side. Nothing found
during the move argued for hoisting it: the park half is still an answer to
"this family executes from the flash being erased", and still has one data
point. The module doc is now the checklist a second family gets.

**`pdb/mod.rs` joins the twin allowlist**, third entry, same shape as
`hal/mod.rs`: core's is the protocol, the family's wires four impls into it.

### B9 — measurement and hardware, stages 1–3f

| Section | Baseline (`59970bc`) | After 3f | Delta |
|---|---:|---:|---:|
| `.text` | 703,736 | 704,432 | **+696** |
| `.rodata` | 195,728 | 196,312 | **+584** |

**+1,280 bytes** against the ≤ ~2 KB budget. Attribution matters more than the
total: +468 is 3b's stop-check crossing the host seam, +200 the A2 assertion,
and +692 is 3d building a `SysmonSample` and then encoding it where the old
code wrote straight to the wire buffer. The first two are correctness bought
deliberately; the third is the price of the encoder being testable at all, and
it buys the golden-bytes test that now guards the layout `tools/pdb` parses.
`platforms/rp/src` is at **7,254 lines**, from 9,494.

**HIL, `testbench_rp2350`.** Per stage, on the real board: 3d exercised every
verb the rewrite touched — ping (the rebuilt greeting, max-PAPK now via the
`PapkFlash` trait), sysmon twice (11 tasks decoding correctly, and the
CPU-delta path reporting IDLE1 at 99.9% on an idle board), installs, input
reaching the handler, and a device-side `STATUS_INCOMPAT` with the device
still answering afterwards. 3f confirmed the task topology byte-identical to
the pre-refactor baseline and three more clean installs, which is what
exercises the park handshake. 3e was verified in the simulator on
`pico_enviro_mon`, the board with both buttons and touch: scripted
tap/swipe/keyevent/back drove navdemo through a full activity cycle
(`onPause` → `onActivityResult req=7 answer=42` → `onRestart`) with no verb
rejected.

**Still owed: the rp2040 half of the stage-3 gate.** Only an rp2350 was
attached for this work. The rp2040 differs where it matters least here (no
tick-freeze busy-wait, so it takes the *simpler* `read_byte_timeout` arm) but
it is also the flash-constrained part, and §4 asks for both.

### B10 — PDB payload layouts are typed now (2026-07-27)

Follow-on work, designed and executed from
`docs/designs/pdb-schema-as-code.md`: the three hand-mirrored payload
layouts (ping greeting, sysmon response, input events) and the keycode name
table moved into `pdb-protocol` as typed encode/decode pairs with golden and
round-trip tests. Affects Stage 7 only as a simplification — the porting
guide's protocol sections can now say "the wire layouts are types in
`pdb-protocol`" instead of describing bytes, and the sysmon golden-bytes
guard named in §7 lives in `pdb-protocol` (still a workspace test).

### B11 — the FreeRTOS simulator added one new residue file (2026-07-28)

`docs/designs/freertos-host-sim.md` landed the same day and made the simulator
run the real kernel. Most of what it touched in `platforms/rp` is where §5 says
it belongs, and one thing is not.

Where it belongs, no action:

- `main.rs` and `glue.rs` — §5, and the new `charge_task_spawn` /
  `release_task_spawn` pair is D6's macro-parameter pattern applied to a second
  leaf, not a new mechanism.
- `boot_budget.rs`'s constants and `BOOT_TASKS` (now carrying a `sim_real`
  flag) — §5's "chip-gated stack policy is this family's memory model".

Already scheduled, so the additions ride along:

- `boot_budget.rs`'s accounting engine grew a tracked-charge/release side and a
  `report_boot_budget` assertion. **Stage 6** already names this split —
  "the accounting engine, including the `black_box` subtlety a re-deriving
  family would get wrong, moves". More moves now than when that was written.
- `fs/mod.rs` gained a `with_fs` arm that submits to the worker task once the
  scheduler is up. **Stage 5 / §3.H** moves the file; the arm goes with it, and
  `spawn_worker()` is already in §3.H's signature list.

**New residue, found and MOVED the same day: `sim_boot.rs`.** Simulator boot
topology — fs worker, background pool, the JVM task, the child-drain wait, and
`start_scheduler` — was written into the family crate while the rest of the
simulator lives in `picodroid-core`. Roughly 80 of its 110 lines named nothing
family-specific, so a second family's simulator would have copied it verbatim:
§0's criterion exactly. No guard caught it — the shadow-twin check compares
filenames across the two trees and this file had no twin.

It now lives at `picodroid-core/src/sim_boot.rs`, taking family policy as a
`BootLeaves` struct of three `fn` pointers — D6's pattern, one level up from
the macro. `platforms/rp/src/glue.rs` gained a ~30-line `run_sim()` holding
the leaves; `platforms/rp/src/sim_boot.rs` is deleted.

Two things worth keeping from how it was done:

- **The JVM task goes through the `Rtos` seam**, via a new `TaskKind::Jvm`.
  That was what made the move cheap rather than hook-heavy: stack sizing and
  the arena charge already flow through `default_stack_bytes` and
  `charge_task_spawn`, so core needed neither a `jvm_stack_words` field nor a
  charge callback — two fields that would otherwise have been permanent. Only
  one exhaustive `match` on `TaskKind` exists in the tree, so the variant cost
  two lines. Device boot still creates its JVM task directly (core affinity
  plus the D4 supervisor loop), but now takes its size from the same place.
- **`BootLeaves::extra_boot_tasks` is the temporary hook**, and it is
  explicitly labelled as such in both the struct and `glue.rs`. Its only
  caller spawns the LittleFS worker. **Stage 5 deletes it**: §3.H moves `fs`
  into this crate with `spawn_worker()` already in its signature list, at
  which point `BootLeaves` drops to two fields and `glue.rs::sim_boot_tasks`
  goes away. That is the one piece of debt this move takes on knowingly.

Recorded because leaving it unrecorded is how §0's list got to four items.
