# Design: less `unsafe`, round 2

**Status: built 2026-10-15.** `unsafe {` 980 → 517, undocumented 713 → 0, `static mut` 81 → 25.
Clippy's `undocumented_unsafe_blocks` is denied in every crate that has unsafe code. Round 1 is
[unsafe-reduction-2026-09.md](unsafe-reduction-2026-09.md); this round started from what it left out.

A prod-readiness review flagged the unsafe surface: 980 blocks, 713 of them without a reason
written down, nearly all in `picodroid-core` and `platforms/rp`. Round 1 had a strict rule of no
change in behaviour, flash or timing, which ruled out the largest lever, a typed LVGL object. This
round takes it. The one behaviour change is in §3.

## 1. Counts

From `scripts/unsafe-baseline.conf`. Each column is a sum over every crate.

| Step | Commit | `unsafe {` | `unsafe fn` | `unsafe impl` | `static mut` | undocumented |
|---|---|---|---|---|---|---|
| start | `35dc8d71` | 980 | 208 | 57 | 113 | 713 |
| A: the counter stops counting `&'static mut T` and string literals | `b1097e82` | 980 | 208 | 56 | 81 | 713 |
| B: picodroid-core outside the widget layer, the small crates | `062d5a30` | 915 | 209 | 50 | 60 | 555 |
| C: `platforms/rp` | `74dc0a43` | 829 | 203 | 45 | 55 | 385 |
| D + E: the widget layer's statics, typed LVGL handles | `946fe8d4` | 517 | 115 | 45 | 25 | 45 |
| F: crate-wide lint, the counter follows clippy's rule | `875ccc02` | 517 | 115 | 45 | 25 | 0 |

The 113 → 81 at step A is the counter, not the code: `static mut` used to match the
`-> &'static mut T` return types too. The last 45 → 0 is also the counter: those blocks had SAFETY
comments that clippy accepts, but the comment began more than three lines above the block.

Per crate, before and after (columns as above):

| Crate | Before | After |
|---|---|---|
| `picodroid-core` | 710 / 129 / 30 / 67 / 487 | 325 / 40 / 25 / 16 / 0 |
| `platforms/rp` | 220 / 41 / 10 / 14 / 195 | 132 / 34 / 4 / 9 / 0 |
| `pd-lvgl-sys` | 0 / 0 / 2 / 0 / 0 | 14 / 3 / 2 / 0 / 0 |
| `jvm` | 17 / 4 / 4 / 0 / 9 | 16 / 4 / 4 / 0 / 0 |
| `pd-install` | 16 / 32 / 5 / 0 / 13 | 15 / 32 / 5 / 0 / 0 |
| `class-link` | 8 / 1 / 4 / 0 / 2 | 6 / 1 / 4 / 0 / 0 |
| `heap4` | 7 / 1 / 1 / 0 / 7 | 7 / 1 / 1 / 0 / 0 |
| `pd-tls` | 2 / 0 / 0 / 0 / 0 | 2 / 0 / 0 / 0 / 0 |

The widget layer (`graphics/lvgl`) went from 487 blocks to about 180, and its `static mut` went to 0.

In `platforms/rp`, 14 of the 88 blocks that are gone were *merged*, not removed. Three sequences of
consecutive register writes share one reason and are now one block each: the I2C timing writes in
`apply_speed!`, the PIO program load in `cyw43_spi_init`, and the PWM `configure_ch!`. Count those
as 3 blocks where there used to be 17, not as code made safe.

## 2. What changed

### picodroid-core outside the widget layer, and the small crates (B)

- **Simulator allocator tests:** an owning `Block` handle that only the test `alloc` can make.
  Fifteen blocks become two.
- **Shared no-allocation helpers:** one `with_env` (`getenv`) and one `raw_stderr`, for the
  simulator allocator and `alloc_ledger`.
- **`safe fn` for C calls with no precondition**, in the `unsafe extern "C" { safe fn … }` form round 1
  used for the HAL seams: `FreeRTOS_socket`, `FreeRTOS_IsNetworkUp`, `FreeRTOS_GetIPAddress`,
  `xTaskGetTickCount`, `uxTaskGetNumberOfTasks` and `picodroid_get_runtime_counter`.
  `vTaskSuspendAll` and `xTaskResumeAll` stay `unsafe`: an unmatched call breaks the kernel's
  invariants, and `configASSERT` is off on the device.
- **Atomics where the access already was a plain load or store:**
  - The simulator display's window and scroll statics are `AtomicU16`.
  - `sched_diag`'s flush flag, sequence counters and config are atomics.
  - `mem_diag`'s native-allocation total is an `AtomicU32`.
  - The serial worker's queue and the sensor sampler's semaphore are write-once `AtomicUsize`s.
  - Every one of these is load and store only. thumbv6m has no compare-and-swap.
- **`OnceLock`, host only:** the simulator's tick timer and its tap-debug flag.
- **Merged `Core0` statics:** `power.rs` goes from 4 statics to 1, and `lifecycle/input.rs`'s three
  `static mut` plus its key-repeat state go into one `Core0`, using copy-in/copy-out.
- **`&'static mut` accessors:** service_lifecycle, alarms and the media driver now go through
  `Core0` or `SectionCell`. That closes round-1 §6's aliasing finding for them.
- **Lint:** `#![deny(clippy::undocumented_unsafe_blocks)]` at the crate roots of jvm, pd-install,
  class-link, heap4, pd-tls and pd-rtos.

### `platforms/rp` (C)

Every interrupt handler compiles to the same instructions as before. `.text` is unchanged on both
`testbench_rp2040` and `testbench_rp2350w`, checked by disassembly after each step.

- **Safe PAC writers instead of 24 raw `.bits()` writes:**
  - A write of 0 becomes `write(|w| w)`.
  - The others use named fields or the `funcsel` enum. PSRAM's XIP_CS1 is column 9 of a PAC enum
    that calls it `GPCK`, and a comment there says so.
  - `.reset()` was tried and not used: it changed LLVM's inlining around `spi::write_raw_start`.
- **SIO pin-mask writers in `hal/rp/periph.rs`:** one documented writer per register. They take
  the caller's `&pac::SIO`; a `steal()` of their own would add the PAC's taken-flag store.
- **DMA helpers in `hal/rp/dma.rs`:** `unsafe fn program`, a safe `request_abort`, and
  `ack_irq0`/`ack_irq1`.
  - `pio_spi`'s `dma_start` took raw addresses in a safe signature and started a channel writing
    to one. It is now `unsafe fn`, and its callers say how they meet the contract.
- **`util::SetOnce<T>`** replaces six per-module cell types and five `unsafe impl Sync`.
  - It holds the same `freertos_rust` values as before, so the interrupt path is unchanged. That
    answers round 1's §4.1 objection.
  - `freertos_rust::Task` is `Send` but not `Sync`, so it is wrapped in a `SharedTask` with one
    documented `unsafe impl Sync`.
- **Init flags and the DMA sink are atomics.** The TRNG buffer sits behind a
  `critical_section::Mutex`: its only caller was already inside one, so no new critical section is
  entered.
- **cyw43 bindings:** one `Driver` token, neither `Send` nor `Sync`, made by an unsafe constructor
  whose contract is "one instance, `init` first and once". All its methods are safe except
  `set_poll_task`.

### The widget layer (D, E)

- **D:** the 37 remaining `static mut` in `graphics/lvgl` move onto `Core0`, one struct per module.
  Zero-initialised parts are kept in `.bss`, so RAM is unchanged.
- **E:** `pd-lvgl-sys` gains the typed handles `Obj`, `Group`, `Indev` and `Display`, an
  `ObjEvent` wrapper, and an `lv_event_cb!` macro.
  - Its C API is now an `unsafe extern "C"` block, with `safe fn` for every function whose
    arguments are only handles, scalars and by-value structs.
  - Creates and NULL-capable getters return `Option`. LVGL returns NULL when a child-array realloc
    fails, so a create can fail.
  - **Still `unsafe`:**
    - The widget-class functions (label, bar, roller, buttonmatrix, textarea, keyboard and the
      rest), because LVGL is built without class checks and an `Obj` does not carry its class.
      Each call cites the Java constructor that fixed the class.
    - Every delete, because `Obj` is `Copy`.
    - Functions that take a C string, a buffer or `user_data`.
- **The liveness argument for `Obj`** (where an `Obj` can come from, why it is live, and the limit
  that makes delete `unsafe`) is written once, on the type in `crates/pd-lvgl-sys/src/lib.rs`.
- **Event callbacks:** the macro centralises one proof obligation that is the same in all 68
  callbacks: LVGL hands an object-event callback a valid event for the duration of the call.
- **`compound.rs`:** a new `widgets/compound.rs` holds the checked-widget code that five widgets
  used to copy.

### Enforcement (F)

- **Lint:** `#![deny(clippy::undocumented_unsafe_blocks)]` is on the crate roots of
  `picodroid-core` and `platforms/rp` and on every other crate with unsafe code. CI's clippy legs,
  which `scripts/pre-commit` mirrors, hold the line.
- **The ratchet's "undocumented" column** is now the text shadow of that lint. A SAFETY anywhere in
  the comment run above the block's statement counts, not only one in the three lines above.

## 3. The behaviour change

Before this round, about 30–50 natives passed a `handle_table::lookup` result straight to LVGL
without a null check. These were the getters and setters of Switch, CheckBox, RadioButton, SeekBar,
ToggleButton, Spinner, EditText, LinearLayout, TextView and Button. LVGL is built with
`LV_USE_CHECK_ARG 0`, so a stale handle there was a null dereference in C:

- in the simulator, a segfault;
- on RP2040/RP2350, a read of boot ROM at address 0.

Every native now starts with `let Some(o) = handle_table::get(id) else { return … }`, so a stale
handle is a no-op. This closes round-1 §6 row 8. The handle sanitizer the simulator runs with still
aborts on a stale handle, by design.

Smaller changes that came with it:

- A failed LVGL create gives handle 0.
- `TextView`'s label and `ProgressBar`'s bar check the object's class first. These are the two
  places where Java cannot vouch for the class.
- `SwipeRefreshLayout` clears its slot when its container is deleted. That fixes the dangling slot
  in round-1 §6 row 5.
- `with_cstr` builds its `&CStr` with `CStr::from_ptr` on a buffer it has just NUL-terminated.
  `from_bytes_until_nul` pulled in 448 B of `memchr`.

## 4. Flash and RAM

`testbench_rp2040` release, measured on the LVGL branch:

| Step | `.text` | `.rodata` | `.data` | `.bss` |
|---|---|---|---|---|
| D | +836 | +104 | −208 | +236 |
| E | +1,564 | 0 | 0 | 0 |

Phase E's growth is spread thin. It is mostly the `None` checks on about 60 create and get paths
that used to hand NULL on, at 4–36 B each. No single lever was worth taking. Phase C is
size-neutral: `.text` is unchanged on both boards, checked by disassembly.

The size ratchet on the merged tree (release helloworld, `scripts/parity-bench.sh --size-only`)
measured this, and it was accepted when the round landed:

| Board | Flash | RAM |
|---|---|---|
| `testbench_rp2040` | +2,768 B (1,002,612) | +40 B |
| `testbench_rp2350` | +932 B (1,472,148) | +0 B |

About 2,300 B of the rp2040 figure is phases D and E. The other ~470 B flash and 12 B RAM were not
measured on their own. Phase B, which no branch measured, is the likely source: its
copy-in/copy-out `Core0` structs are a small codegen change.

## 5. Left out, and why

| What | Why |
|---|---|
| `MemRegion` safe methods (pd-install) | The package directory and the interpreter hold `&'static [u8]` into the region, so a safe erase would mutate memory under live borrows. Separately, `MemRegion::restore` is already a safe mutator. |
| `std::sync` locks in the simulator's touch and app-region code | Round 1's caveat: a lock between FreeRTOS tasks on the POSIX port can deadlock on priority. |
| heap4's arena as a slice | It would add bounds checks inside the global allocator. |
| The simulator's `usize` handle seams (`rtos.rs`, `net.rs`), glue.rs's RTOS handle casts | `pd-rtos` hands out raw `usize` handles through safe methods. Collapsing the casts would hide that rather than fix it. Typed handles in `pd-rtos` are the fix. |
| jvm's `transmute(usize → fn)` | The alternative is a link-time seam, which is a direct call instead of an indirect one: a timing change. |
| The pdb USB counters as atomics | LLVM would reorder them inside `USBCTRL_IRQ`. |
| `vTaskSuspendAll` / `xTaskResumeAll` as `safe fn` | See §2. |
| Crate-wide `unsafe_op_in_unsafe_fn` | It would *add* a block to the body of each of the 115 `unsafe fn`. That is worth doing with the edition-2024 move, not here. |
| The cross-core seqlocks (`hal/wifi.rs`, `event_ring`, `sched_diag`'s print copy), `boot.rs` `shared_heap` / `shared_jvm` / `publish_jvm`, `packages::dir()`, the native handler's GC roots | Documented, not restructured. (`shared_heap` has since been tied to the run-lock guard: UR-5, 2026-10-15.) |

## 6. Findings

Found during this round. None is fixed here. Each one is a behaviour change, so each gets its own
decision and its own commit.

The backlog, with failure scenarios and fix options for these and for round 1's still-open rows, is
[bugs-unsafe-review-2026-10.md](../bugs-unsafe-review-2026-10.md).

| Where | What |
|---|---|
| `platforms/rp/src/hal/rp/pio_spi.rs`, abort | If `abort_dma`'s spin times out, `cyw43_spi_transfer` returns −1 while the RX channel may still be writing into the caller's buffer. This is the most serious one. |
| `platforms/rp/src/hal/rp/spi/mod.rs`, `transfer_raw` | An `rx` shorter than `tx` overruns in the interrupt handler (round-1 §6, still open; the SAFETY comment says so). |
| `spi_state`, `display()`, `touch()` in `platforms/rp` | A safe function returning `&'static mut`. A GT911 board's raw touch read on the UI task while the sampler runs would alias. Today only XPT2046 calibration calls it. Fixed 2026-10-15 as UR-5 in [../bugs-unsafe-review-2026-10.md](../bugs-unsafe-review-2026-10.md): an all-atomic transfer state behind `&`, the two drivers behind a mutex with closure-scoped access, and `shared_heap` borrowed from the run-lock guard. |
| SPI and I2C bus init | Two tasks initialising the same bus at once race the check-then-store. `JVM_TASK` is protected from the pdb task only by timing. |
| `picodroid_port_entropy_bytes` | A safe C entry point that dereferences a raw pointer. |
| `heap4::Heap4::free` | A safe public function. A misaligned or mid-block offset corrupts the free list. |
| `jvm/src/parity.rs`, `slowest_native` | Parity-metrics builds only. A read racing `count_native` can pair one name's pointer with another's length. |
| Simulator `net.rs`, `deref_socket` | A reader blocked in `recv` and a sender on the same socket hold two `&mut SimSocket`. |
| `pd-install`, `install` / `PAGE_BUF` | A safe public function on a static buffer. It is sound only because the debug-bridge task is its one caller. |
| `graphics/lvgl/view_ops.rs:190`, `addView` | `lv_obj_set_parent` has no cycle check, so Java can make a parent cycle. The call stays `unsafe`, with an OPEN note. Fixed 2026-10-15 as UR-3 in [../bugs-unsafe-review-2026-10.md](../bugs-unsafe-review-2026-10.md): an ancestor walk refuses the parent with `Reparent::Cycle`. |
| `graphics/lvgl/window.rs:170` | `window::sync` deletes the old compat window with a raw `lv_obj_delete`, bypassing keyboard unbind and animation cancel. A textarea bound inside it could leave `bound_ta` dangling. Fixed 2026-10-15 as UR-4 in [../bugs-unsafe-review-2026-10.md](../bugs-unsafe-review-2026-10.md): the delete goes through `view_ops::delete_obj`. |
| `graphics/lvgl/widgets/keyboard.rs` | The keyboard reset leaks the system keyboard object. |
| NVIC IPR byte writes, `ep1_in_pid` | Still open from round 1. `periph::enable_irq`'s doc now names the IPR one. `ep1_in_pid` fixed 2026-10-15 as UR-6 in [../bugs-unsafe-review-2026-10.md](../bugs-unsafe-review-2026-10.md). |

## 7. Verification

- **Each of the three branches, per commit:** `./scripts/pre-commit` (the guards, fmt, and clippy
  on the simulator and all six board legs).
- **The merged tree:**
  - `./scripts/test.sh` in both shrink modes.
  - The helloworld simulator smoke.
  - `./scripts/pre-commit --full`: the legacy-handle-cast, mem-diag and sched-diag legs and the
    size ratchet on both boards.
- **Simulator runs:**
  - B: threadparity, qa_thr, threadstress, jsondemo, helloworld with `--mem-diag` and
    `--sched-diag`, and the sched-diag self-test on animdemo.
  - D and E (handle sanitizer on): qa_ui, dialogdemo, keydemo, swipedemo, animdemo, bugbash_ui,
    callbacktest, qa_life, navdemo, fragmentdemo, keynav, picoclock, picoenvmon, menudemo,
    pagerdemo, imagedemo and layoutdemo. pickerdemo and snackbardemo were driven headless.
- **Release images:** testbench_rp2040 and testbench_rp2350w, size and disassembly, after each
  phase C step.
- **Hardware**, 2026-10-15, `hil-run.sh --mode no-shrink --include-hw`, one row at a time on the
  merged tree. Every row that ran passed; no row failed.
  - **`testbench_rp2040`:** helloworld (the pdb rows SKIP on single-app firmware), bugbash_ui,
    dialogdemo, animdemo, callbacktest, navdemo, qa_life, threadstress, filesdemo, adcdemo,
    i2cdemo, pwmdemo and spidemo. qa_ui and qa_io SKIP on rp2040.
  - **`testbench_rp2350w`:** the same rows plus qa_ui and the cyw43 and PIO SPI rows netdemo,
    http_get and https_get.
  - **`pico_touch_kit`** (PSRAM, GT911): helloworld, bugbash_ui, dialogdemo, animdemo,
    callbacktest, navdemo, qa_life, threadstress and filesdemo. qa_ui SKIPs on the touch kit.
  - The shrink side and the rest of the matrix are left to the nightly.
