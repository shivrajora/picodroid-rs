# Design: less `unsafe`, same firmware

**Status: phases 0–7 built, 2026-09-29.** `unsafe {` 1,197 → 919, `static mut` 243 → 113; the
execution log at the bottom has the per-phase counts, and §4.1 says what was planned and left out.

Started 2026-09-28. The rule for every phase: no change in behaviour, flash, RAM or
timing. Anything that would change behaviour, including fixes for the soundness findings
in §6, is a separate decision and a separate commit.

## 1. Where the `unsafe` is

Counts at `b83170cc`, from `scripts/check-source-guards.sh --unsafe` (text counts over
non-comment lines, vendored code excluded):

| Crate | `unsafe {` | `unsafe fn` | `unsafe impl` | `static mut` |
|---|---|---|---|---|
| `picodroid-core` | 839 | 120 | 30 | 223 |
| `platforms/rp` | 277 | 37 | 12 | 17 |
| `pd-rtos` | 30 | 0 | 0 | 0 |
| `jvm` | 17 | 4 | 4 | 0 |
| `pd-install` | 14 | 28 | 5 | 2 |
| `class-link`, `heap4`, `papk-format`, `pd-tls`, `pd-lvgl-sys` | 20 | 2 | 7 | 1 |
| the other eleven crates | 0 | 0 | 0 | 0 |

Inside the two large crates it is a few patterns repeated many times:

| Pattern | Where | Sites |
|---|---|---|
| One-line wrappers over link-time `extern "Rust"` seams | `hal/facade.rs`, `pd-rtos`, `host.rs` | 117 |
| `static mut` listener maps reached through `map_mut(&raw mut X)` | `graphics/lvgl` | ~100 |
| Hand-rolled ring queues: an array, a `_HEAD` and a `_TAIL` | `graphics/lvgl` | 25 queues |
| Scalar `static mut` state, and array + length pairs | `graphics/lvgl` | ~45 statics |
| `XxxCell(UnsafeCell<T>)` + `unsafe impl Sync`, redefined per module | core and `platforms/rp` | ~30 types |
| `pac::Peripherals::steal()` and raw register bits | `platforms/rp/src/hal/rp` | ~170 |
| Direct LVGL calls | `graphics/lvgl` | ~690 calls |

The JVM's interpreter, garbage collector and object heap contain none.

## 2. Constraints

- **thumbv6m has no compare-and-swap.** Atomics are load and store only, never `SeqCst`.
- **The LVGL-layer state is task-confined, not interrupt-shared.** Every widget queue and
  listener map is written and read by JVM tasks, all pinned to core 0
  (`platforms/rp/src/task_affinity.rs`). Input injection goes through `hal::gpio::inject`
  and the touch override, never into a widget queue. So the right tool is `Cell`, and an
  atomic there would advertise a cross-task safety nobody relies on.
- **Garbage collection visits the listener maps from whichever JVM task collects**, so the
  justification is the core-0 pinning, not "the UI task".
- **`AtomicSection` is `vTaskSuspendAll` through a function pointer.** It is never added to
  state that takes no lock today, and it excludes neither an interrupt nor core 1.
- **No file at the same relative path in both `src` trees.** Shared primitives live in
  `crates/picodroid-core/src/util/`.

## 3. Shared primitives

| Type | File | What it replaces |
|---|---|---|
| `Core0<T>` | `util/local.rs` | The per-access `unsafe` on task-confined statics. One `unsafe impl Sync`, one `unsafe` per static at its declaration. |
| `LocalRing<T, N>` | `util/local_ring.rs` | The 25 array + head + tail queues. Same semantics: `N - 1` usable slots, the new element is dropped when full. |
| `LocalSet<T, N>` | `util/local_set.rs` | Array + length pairs. |
| `SectionCell<T>` | `util/section_cell.rs` | Cells whose every access already sits inside an `AtomicSection`. `get` borrows the guard exclusively, so the reference cannot outlive the section. |
| `periph::steal`, `periph::enable_irq` | `platforms/rp/src/hal/rp/periph.rs` | 46 `Peripherals::steal()` sites and 7 copies of the NVIC priority write. |
| `PtrMap<N>` on `Cell` | `graphics/lvgl/listener_map.rs` | `map_mut` / `map_ref`, which go away. |

## 4. Phases

| # | Change | `unsafe {` removed |
|---|---|---|
| 0 | The ratchet and its baseline, `#![forbid(unsafe_code)]` on the eleven clean crates, a handful of needless transmutes | 6 |
| 1 | Seams declared `unsafe extern "Rust" { safe fn … }` | 117 |
| 2 | `Core0` and the `Cell`-based `PtrMap` | 39 |
| 3 | `LocalRing` for the 24 widget queues | 27 |
| 4 | 27 scalars into `Core0<Cell<T>>`, three pointer registries onto `LocalSet` | −2 |
| 5 | `SectionCell` for the thread table, the monitor store, the JSON pool and pdb's child-task list | 29 |
| 6 | `platforms/rp`: one `steal()`, one `enable_irq()`, the PAC's enumerated `funcsel` setters | 58 |
| 7 | Host-only aligned buffers in `papk-format` and `pd-install`; `papk-format` takes `forbid(unsafe_code)` | 4 |

The block counts for phases 2–4 are net of what `Core0` costs: `Core0::new` is an `unsafe
fn`, so each of the 75 converted statics carries one `unsafe` block at its declaration, with
its reason, where every access used to carry one. Phase 4 comes out at −2 for that reason:
its scalars were mostly read inside blocks that also call LVGL, so the blocks stayed. What
it removed is 34 `static mut`.

### 4.1 Planned, and left out

| What | Why |
|---|---|
| `SetOnce` for the SPI, I2C, DMA and USB semaphore cells in `platforms/rp` | They hold `freertos_rust::Semaphore` values. Moving them onto the seam's raw handles changes the call path inside interrupt handlers, which is a timing change and cannot be checked in the simulator. |
| `alarms.rs` onto `SectionCell` | Its table and horizon are borrowed together under one section; two blocks were not worth restructuring it. |
| `Peripherals::steal()` replaced by per-peripheral `steal()` | It would drop the PAC's flag store, which makes the image differ. `periph::steal()` keeps the call as it was. |
| The simulator HAL (sockets, display statics, `getenv`) | Host-only, and a `std::sync` lock between FreeRTOS tasks on the POSIX port can deadlock on priority; each wants its own look. |
| `unsafe_op_in_unsafe_fn` crate-wide on the two large crates | 155 `unsafe fn` bodies to annotate. The new modules carry it; the ratchet's "undocumented" column is what drives the rest down. |

Left alone on purpose, to be documented and nothing more: the XIP-off flash path, core 1
parking, QMI timing, PSRAM bring-up, the cortex-m-rt exception handlers, the C-ABI exports
for cyw43 and FreeRTOS, the simulator's `GlobalAlloc`, the `unsafe trait` seams, the widget
handle table, and the JVM's `StringTable` and `ClassFile`.

Wrapping the LVGL call surface behind a safe object type would remove about 200 more
blocks. It adds null checks and flash, so it changes behaviour and is not part of this work.

## 5. The ratchet

`scripts/check-source-guards.sh --unsafe` counts, per crate, `unsafe {`, `unsafe fn`,
`unsafe impl`, `static mut`, and the `unsafe {` blocks with no `SAFETY` on their line or the
three above. It compares them with `scripts/unsafe-baseline.conf` and fails on a rise. It
also fails on a fall the baseline has not caught up with, so the baseline is always the
tree's own count; `--unsafe-accept` rewrites it. It runs in `scripts/pre-commit` and in
CI's guards job.

## 6. Findings that are not part of this work

Found while surveying. Each is a behaviour change to fix, so none is fixed here.

| Where | What |
|---|---|
| `platforms/rp/src/main.rs`, `vApplicationMallocFailedHook` and `vApplicationStackOverflowHook` | `#[no_mangle] fn` without `extern "C"`, called from C. The idle hooks beside them have it. |
| `platforms/rp/src/hal/rp/pdb_usb/mod.rs`, `ep1_in_pid` | Read-modify-write in task context with interrupts unmasked; `USBCTRL_IRQ` writes the same field. |
| `crates/picodroid-core/src/executors/main_queue.rs`, `ShadowCell` | A `Cell<bool>` written from two tasks. |
| Seven byte writes to the NVIC priority registers at `0xE000_E400` | ARMv6-M allows word access only, so the priority is probably not applied on the RP2040. |
| `graphics/lvgl/widgets/swipe_refresh_layout.rs`, `SLOTS` | Never cleared when the container is deleted; `time_picker.rs` has the fix. Not re-verified. |
| `platforms/rp/src/hal/rp/spi/mod.rs`, `transfer_raw` | An `rx` shorter than `tx` overruns in the interrupt handler. Today's callers pass equal lengths. Not re-verified. |
| `boot.rs` `shared_heap()` and 18 other safe functions returning `&'static mut T` | Two calls give two aliasing exclusive references. |
| About 50 call sites in `graphics/lvgl` | An unchecked `handle_table::lookup` result goes to LVGL, which is built with `LV_USE_CHECK_ARG 0`. |

### 6.1 The seam declarations are not checked against their definitions

`hal/facade.rs` says the test build catches a declaration that drifts from its definition,
through `clashing_extern_declarations`. It does not. With `__pd_hal_gpio_read` declared as
taking an extra argument, `cargo check -p picodroid-core --tests` compiled without a
diagnostic (2026-09-29): that lint compares foreign declarations with each other, not with a
`#[no_mangle]` definition. The definition side is still held to the trait. The declaration
side is held by nothing, and it is where `safe fn` now puts its trust. A test that parses
both files and compares signatures would close it.

## 7. Execution log

Totals over every crate, from `scripts/unsafe-baseline.conf` at each commit.

| Phase | Commit | `unsafe {` | `static mut` | `unsafe impl` | undocumented |
|---|---|---|---|---|---|
| start | `b83170cc` | 1,197 | 243 | 58 | 1,036 |
| 0 | `1dca84e6` | 1,191 | 243 | 58 | 1,030 |
| 1 | `beea6c3d` | 1,074 | 243 | 58 | 913 |
| 2 | `25f7a50f` | 1,035 | 221 | 59 | 853 |
| 3 | `1bf2e525` | 1,008 | 149 | 59 | 802 |
| 4 | `5f9b6ea1` | 1,010 | 115 | 59 | 777 |
| 5 | `6ad11aa4` | 981 | 113 | 56 | 775 |
| 6 | `8ffb240a` | 923 | 113 | 56 | 718 |
| 7 | this commit | 919 | 113 | 56 | 718 |

Checked per phase: clippy on every board and the simulator, the host tests, the helloworld
smoke. Phases 2–4 also ran `qa_ui`, `dialogdemo`, `keydemo`, `swipedemo`, `animdemo`,
`bugbash_ui`, `callbacktest`, `qa_life` and `navdemo` in the simulator with the handle
sanitizer on; phase 5 ran `threadparity`, `qa_thr`, `threadstress`, `threaddemo` and
`jsondemo`. Phase 6 cannot be seen in the simulator: its release images for
`testbench_rp2040` and `testbench_rp2350w` are the same size as before and disassemble to
the same instructions.
