# Design: less `unsafe`, same firmware

**Status: in progress.** The execution log at the bottom says which phases have landed.

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
| `SectionCell<T>` | `util/section_cell.rs` | Cells whose every access already sits inside an `AtomicSection`. |
| `SetOnce` | `util/set_once.rs` | Write-once RTOS handles, after `SemCell` in `platforms/rp/src/hal/rp/gpio.rs`. |
| `PtrMap<N>` on `Cell` | `graphics/lvgl/listener_map.rs` | `map_mut` / `map_ref`, which go away. |

## 4. Phases

| # | Change | `unsafe {` removed |
|---|---|---|
| 0 | The ratchet and its baseline, `#![forbid(unsafe_code)]` on the eleven clean crates, a handful of needless transmutes | ~10 |
| 1 | Seams declared `unsafe extern "Rust" { safe fn … }` | 117 |
| 2 | `Core0` and the `Cell`-based `PtrMap` | ~95 |
| 3 | `LocalRing` | ~60 |
| 4 | Scalars and `LocalSet` | ~40 |
| 5 | `SectionCell` and `SetOnce`, for cells already under a section | ~50 |
| 6 | `platforms/rp`: one `pac` alias with per-peripheral accessors, one interrupt-enable helper, the PAC's enumerated `funcsel` setters | ~50 |
| 7 | Simulator and host tools | ~25 |

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

## 7. Execution log

| Phase | Commit | `unsafe {` after | `static mut` after |
|---|---|---|---|
| start | `b83170cc` | 1,197 | 243 |
