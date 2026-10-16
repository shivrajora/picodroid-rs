# Bug backlog: soundness findings from the unsafe review — 2026-10-15

**Status: open, except UR-1 to UR-8 (fixed 2026-10-15).** Every item is a memory-safety or concurrency defect found
while reading unsafe code during the two unsafe-reduction rounds:
[designs/unsafe-reduction-2026-09.md](designs/unsafe-reduction-2026-09.md) §6 and
[designs/unsafe-reduction-2026-10.md](designs/unsafe-reduction-2026-10.md) §6.

Those rounds were not allowed to change behaviour, apart from the typed LVGL handle. So each finding
was written down at its site (a SAFETY comment or an OPEN note) and left alone. Each one is its own
decision and its own commit.

All of them were found by reading code. None has been seen on a board or reproduced in the
simulator. Where it isn't obvious, a "Verify" line says how to make an item fire or check its fix.
Line numbers are at `f7652f0d`. The sections are grouped as device, then framework; the summary
table is in severity order.

## Summary

| Id | Severity | Where | One line |
|---|---|---|---|
| [UR-1](#ur-1) | high — **fixed** | `platforms/rp/src/hal/rp/pio_spi.rs` | A cyw43 transfer whose DMA abort times out returns while the RX channel may still be armed on the caller's buffer |
| [UR-2](#ur-2) | high — **fixed** | `platforms/rp/src/hal/rp/spi/mod.rs` | `transfer_raw` with `rx` shorter than `tx` overruns `rx` in the interrupt handler |
| [UR-3](#ur-3) | medium — **fixed** | `graphics/lvgl/view_ops.rs` | `addView` into the view's own subtree makes an LVGL parent cycle |
| [UR-4](#ur-4) | medium — **fixed** | `graphics/lvgl/window.rs`, `widgets/keyboard.rs` | `window::sync` deletes the old window with a raw delete, so the keyboard's bound textarea can dangle |
| [UR-5](#ur-5) | medium — **fixed** | `platforms/rp` `spi_state`, `display()`, `touch()`; `boot.rs` `shared_heap` | Safe functions return `&'static mut`, so two calls alias |
| [UR-6](#ur-6) | medium — **fixed** | `platforms/rp/src/hal/rp/pdb_usb/mod.rs` | `ep1_in_pid` is written by the task and by `USBCTRL_IRQ` with interrupts unmasked |
| [UR-7](#ur-7) | medium — **fixed** | `platforms/rp/src/main.rs` | The FreeRTOS malloc-failed and stack-overflow hooks are called from C but are not `extern "C"` |
| [UR-8](#ur-8) | medium — **fixed** | `platforms/rp/src/hal/rp/periph.rs` | NVIC priority is written a byte at a time; ARMv6-M allows word access only |
| [UR-9](#ur-9) | low — **fixed** | `platforms/rp/src/hal/rp/entropy.rs` | `picodroid_port_entropy_bytes` is a safe C entry point that writes through a raw pointer |
| [UR-10](#ur-10) | low — **fixed** | `spi/mod.rs`, `i2c/mod.rs`, `pdb/pending.rs` | Bus init is check-then-store; `JVM_TASK` is protected from the pdb task by timing only |
| [UR-11](#ur-11) | low | `graphics/lvgl/widgets/keyboard.rs` | An app reload forgets the system keyboard without deleting it, so each reload leaks one |
| [UR-12](#ur-12) | low (sim) | `crates/heap4/src/lib.rs` | `Heap4::free` is safe and public; a misaligned or mid-block offset corrupts the free list |
| [UR-13](#ur-13) | low (sim) | `hal/sim/net.rs` | A reader and a sender on one socket hold two `&mut SimSocket` |
| [UR-14](#ur-14) | low | `crates/jvm/src/parity.rs` | `slowest_native` can pair one name's pointer with another's length |
| [UR-15](#ur-15) | low | `crates/pd-install/src/orchestrator.rs` | `install` is safe and public over a static page buffer; it is sound only with one caller |

"Severity" estimates how bad it would be if it fired and how reachable it is today. It is not a
measured rate.

## Device

### UR-1

**A cyw43 transfer whose DMA abort times out can leave DMA armed on a buffer the caller has taken
back.**

- **Where:** `platforms/rp/src/hal/rp/pio_spi.rs`, `abort_dma` (:277) and its callers in
  `cyw43_spi_transfer` (:678, :733).
- **What:** when a wait fails, the transfer calls `abort_dma`. That writes `CHAN_ABORT` and waits a
  bounded time for each channel's `BUSY` to clear. On timeout it logs `DMA channel N did not abort`
  and returns, and the transfer returns −1. The RX channel's write address is still the caller's
  `rx` buffer (or `BOUNCE`), and the channel may still be live.
  - The cyw43 driver reuses or frees that buffer as soon as the call returns.
  - The state machine is disabled right after, so the RX FIFO stops filling and the stale channel
    should stall on its DREQ. But the next transfer re-enables the SM and re-programs the same
    channels. If the stale channel is still active, it can take RX FIFO words meant for the new
    transfer and write them into the old buffer.
- **When it fires:** only after a DMA wait has already failed (a wedged DREQ), which is itself a
  fault path. That makes it rare, but it is a write into memory the caller no longer owns.
- **Fix options:**
  - When the abort does not retire, stop returning: mark the bus dead, keep it unusable, and log.
  - Or reset the DMA block through `RESETS` before the next transfer.
  - The point either way is that a channel that did not abort is never reused and never left
    aimed at a caller's buffer.
- **Verify:** force the timeout (lower the spin bound in a test build, or hold the PIO SM stalled)
  and check that the next transfer doesn't run on the stale channel.
- **Fixed 2026-10-15** (first option). `abort_dma` pauses a channel that did not retire (clears
  `EN`, so it issues no transfer even if its DREQ asserts) and sets `BUS_DEAD`; every later
  `cyw43_spi_transfer` returns −1 before the bounce copy, the SM enable, the FIFO flush or any
  channel programming. The SM is already left disabled at the transfer tail, so the RX DREQ never
  asserts again. The bus stays retired until reboot: freeing the channel would take a reset of the
  DMA block, which the display's channels share. Wi-Fi is lost for that boot, logged once at
  `error`. Not reproduced on a board; build-checked only.

### UR-2

**`transfer_raw` with `rx` shorter than `tx` overruns `rx` in the interrupt handler.**

- **Where:** `platforms/rp/src/hal/rp/spi/mod.rs`, `transfer_raw` (:672).
- **What:** the ISR path stores `tx.len()` as the transfer length and writes received bytes through
  `rx_ptr` up to that length. Nothing checks `rx.len() >= tx.len()`.
  - The small-transfer polling path has the same assumption.
  - The SAFETY comment at the ISR records it.
- **Today:** every caller passes equal lengths, so it is latent. But `transfer_raw` is a safe
  `pub fn`, so safe code can overrun memory with it.
- **Fix:** assert or clamp at entry (`let n = tx.len().min(rx.len())`), or take one `&mut [u8]` transferred
  in place, as embedded-hal's `SpiBus::transfer_in_place` does. Add a host test of the length contract if
  the function moves behind the HAL trait.
- First recorded in round-1 §6.
- **Fixed 2026-10-15.** The transfer state carries `rx_len` beside `len`, and both paths store a
  received byte only while it fits (`xfer::stores_rx`, host-tested): every `tx` byte is still
  clocked and counted, so the transfer ends on the count, but nothing is written past `rx.len()`.
  The polling path indexes `rx` with `get_mut`. The contract (lengths may differ, replies past
  `rx` are dropped, a longer `rx` keeps its tail) is on `HalSpi::transfer_raw`; the sim already
  behaved that way. `RpSpiBus::transfer` adds embedded-hal's other half, clocking zeros to fill
  a `read` longer than `write`. Not reproduced on a board; build-checked and unit-tested only.

### UR-5

**Safe functions that return `&'static mut`.**

- **Where:**
  - `platforms/rp/src/hal/rp/spi/mod.rs` `spi_state` (:122)
  - `platforms/rp/src/hal/rp/display.rs` `display()` (:98)
  - `platforms/rp/src/hal/rp/touch.rs` `touch()` (:220)
  - `crates/picodroid-core/src/boot.rs` `shared_heap()` (:56)
- **What:** each hands out a `&'static mut` to a static from a safe function, so two calls give two
  live exclusive references, which is UB whether or not they are used at once.
- **Concrete case:** on a GT911 board, a raw touch read on the UI task while the touch sampler task
  runs would alias the `Touch`. Today only XPT2046 calibration calls the raw read.
- **Fix:** the round-2 pattern. Move each onto `Core0<Cell<…>>`, `SectionCell` (borrow under the
  section guard) or `SetOnce`, and hand out `&T` with interior mutability or a closure-scoped
  borrow (`with_display(|d| …)`).
- Round 2 converted service_lifecycle, alarms, the media driver and `hw_scroll`. These are what's
  left.
- **Fixed 2026-10-15.** No safe function returns `&'static mut` any more; each site got the shape
  its sharing called for.
  - `spi_state` returns `&'static SpiXferState`. The record is atomics throughout (`AtomicPtr`,
    `AtomicUsize`, an `AtomicU8` for the kind), reached through `&` from the task and the handler
    alike, so the waiter's look at the counters while the handler runs is a relaxed load, not a
    second `&mut`. The handler reads the description into locals on entry and stores the counters
    back at each exit; `start_isr_xfer!` puts a release fence between the task's stores and the
    unmask. `XferCell` and its `unsafe impl Sync` are gone. Relaxed loads and stores are the same
    `ldr`/`str` the integer fields compiled to.
  - `display()` and `touch()` became `with_display(|d| …)` and `with_touch(|t| …)` over a
    `SetOnce<Mutex<_>>` (a FreeRTOS mutex, `freertos_rust::Mutex`): a borrow lasts one closure and
    a second caller waits. The doc's concrete case, a raw touch read on the UI task while a GT911
    sampler runs, now serialises. Neither closure re-enters its module (`yield_display_bus` runs
    before the take). Uncontended, one take and one give per call; the flush's three calls per band
    sit beside a 1.4 ms DMA write.
  - `shared_heap(&mut Held) -> &mut SharedJvmHeap`: the heap is borrowed from the run-lock guard,
    for as long as the guard is borrowed, so no code reaches it without holding the lock and no
    task holds two borrows at once (the shape of `SectionCell::get`). The three callers
    (`run_app`, the background worker, `Thread.start`) acquire first and borrow second. What the
    type still does not say is that the interpreter gives the lock up around blocking waits while
    the borrow is live; that is the run lock's documented contract, unchanged here.
  - Build-checked on every clippy leg, unit-tested (`xfer::begin` and the kind's `u8` round trip),
    sim-smoked; not run on a board. The unsafe baseline fell by 5 blocks, 1 impl and 2 `static mut`
    in `platforms/rp`.

### UR-6

**`ep1_in_pid` is a read-modify-write race between the pdb task and `USBCTRL_IRQ`.**

- **Where:** `platforms/rp/src/hal/rp/pdb_usb/mod.rs`. The field is declared at :106; the module
  note at :92–101 names it.
- **What:**
  - `write_bytes` (task context, interrupts unmasked) toggles `USB.ep1_in_pid` for each EP1 IN
    packet.
  - `USBCTRL_IRQ` clears it on a bus reset (:314, :448).
  - If the reset lands between the task's read and its write, the task writes back the
    pre-reset PID. The next IN packet then goes out with the wrong DATA0/DATA1 toggle, and the host
    drops it as a duplicate.
- **When it fires:** a bus reset during a pdb reply. That is rare, but it loses a reply silently.
- **Fix:** mask `USBCTRL_IRQ` (or take a `critical_section`) around the toggle in `write_bytes`.
- First recorded in round-1 §6.
- **Fixed 2026-10-15** (the mask). `write_bytes` runs each packet's buffer copy, toggle
  read-modify-write and buffer-control store inside `cortex_m::interrupt::free`, the same section
  `on_byte_consumed` already uses on the RX side; the pdb task is pinned to core 0, where
  `USBCTRL_IRQ` is unmasked, so PRIMASK on that core holds the handler off. A bus reset that
  arrives during the section is taken when it ends and clears the toggle after the task's
  write-back, so the next IN packet goes out as DATA0 as the host expects. The section is one
  64-byte copy and three word stores per packet. The module's USB-rule note no longer carries the
  unmasked-field exception. Not reproduced on a board; build-checked only.

### UR-7

**The malloc-failed and stack-overflow hooks use the Rust ABI but are called from C.**

- **Where:** `platforms/rp/src/main.rs`, `vApplicationMallocFailedHook` (:238) and
  `vApplicationStackOverflowHook` (:275), both `#[no_mangle] fn` without `extern "C"`. The idle
  hooks beside them have it.
- **What:** FreeRTOS calls them through the C ABI. With no arguments the two ABIs happen to agree
  on Cortex-M, so the malloc hook works by accident. The stack-overflow hook takes two pointer
  arguments, and the Rust ABI makes no promise about them.
- **When it fires:** only on a heap or stack failure, which is when a reliable report matters most.
- **Fix:** add `extern "C"` to both. That is a one-word change. Confirm in the disassembly that the
  argument registers are as expected.
- First recorded in round-1 §6.
- **Fixed 2026-10-15.** Both hooks are `extern "C"` now, the same shape as the idle hooks beside
  them. The kernel's declarations (`portable.h`, `task.h`) are unchanged, so the symbols link as
  before; the stack-overflow hook's two arguments now arrive in `r0`/`r1` by contract, not by the
  Rust ABI happening to agree. The bodies are untouched: the malloc hook stays a no-op and the
  overflow hook still breakpoints and parks. Build-checked on every clippy leg and sim-smoked; not
  run on a board.

### UR-8

**NVIC priorities are written with byte stores, which ARMv6-M does not define.**

- **Where:** `platforms/rp/src/hal/rp/periph.rs`, `enable_irq`. The doc at :37 and the store
  through `0xE000_E400 as *mut u8` at :47.
- **What:** the Cortex-M0+ (RP2040) supports only word access to `NVIC_IPR`. A byte store there is
  UNPREDICTABLE, and on the M0+ it probably leaves the priority unset. Interrupts then run at
  whatever priority they reset to, not the one the driver asked for. The Cortex-M33 (RP2350)
  allows byte access, so this is RP2040-only.
- **Fix:** read-modify-write the containing word. Better, call `cortex_m::peripheral::NVIC::set_priority`,
  which already does the word access on v6-M.
- **Verify:** read `NVIC_IPR` back over SWD on a `testbench_rp2040` after boot.
- First recorded in round-1 §6.
- **Fixed 2026-10-15** (the `NVIC::set_priority` call). `enable_irq` sets the priority through
  `cortex_m::Peripherals::steal().NVIC`, which stores the byte on ARMv7-M and ARMv8-M and
  read-modify-writes the containing IPR word on ARMv6-M. The call runs under
  `cortex_m::interrupt::free` so a task switch cannot land between the read and the write and lose
  a neighbouring interrupt's byte; the unmask moved into the same section. The `testbench_rp2040`
  debug disassembly shows every site (USB, SPI0, SPI1, DMA_IRQ_0, I2C0, I2C1) as a word `ldr`,
  mask, `str` on its IPR word, with no byte store to `0xE000_E4xx` left. The value is unchanged
  (0x10, `configMAX_SYSCALL_INTERRUPT_PRIORITY`); on the M0+ only the top two bits of each byte
  are implemented, so it reads back as 0, and the RP2040 port's critical sections use PRIMASK, so
  the priority never decided what a critical section masks there. Not read back over SWD;
  build-checked on every clippy leg and sim-smoked.

### UR-9

**`picodroid_port_entropy_bytes` is a safe function that dereferences a raw pointer.**

- **Where:** `platforms/rp/src/hal/rp/entropy.rs` :62, which has
  `#[allow(clippy::not_unsafe_ptr_arg_deref)]`.
- **What:** it is a `pub extern "C" fn`, not `unsafe`. It builds a slice from `buf`/`len` after a
  null check. A Rust caller could pass any pointer from safe code. The only real caller is the TLS
  stack in C, which passes a valid buffer.
- **Fix:** make it `pub unsafe extern "C" fn` with a `# Safety` section. Nothing in Rust calls it,
  so no call site changes.
- **Fixed 2026-10-15.** Both definitions, the RP family's and the simulator's in
  `hal/sim/platform.rs`, are `pub unsafe extern "C" fn` with the contract (`buf` null, or `len`
  writable bytes nothing else touches during the call) in a `# Safety` section; the
  `not_unsafe_ptr_arg_deref` allows are gone. The one caller is the `extern "C"` declaration in
  core's `hal::entropy`, already called inside `unsafe`, so no call site changed.

### UR-10

**Bus and task initialisation is check-then-store.**

- **Where:**
  - `platforms/rp/src/hal/rp/spi/mod.rs` :552.
  - `platforms/rp/src/hal/rp/i2c/mod.rs` :272.
  - `platforms/rp/src/pdb/pending.rs` `JVM_TASK` (:54, set at :67, read at :94).
- **What:**
  - Two tasks initialising the same SPI or I2C bus at once would both see "not initialised" and
    both build the semaphore and program the block.
  - `JVM_TASK` is set by the JVM task and read by the pdb task. The ordering comes from boot
    timing, not from the type: `SetOnce::set` is an `unsafe fn` whose contract (no reader yet) holds
    only because of when the pdb task starts.
- **Today:** bus init runs from the board's boot sequence and the sensor sampler, which don't
  overlap, so nothing races.
- **Fix:** do every bus init in boot before the scheduler starts, which turns the "one initialiser"
  assumption into structure. Or guard init with a `critical_section` (one per bus, boot only, so
  it's not hot).
- **Fixed 2026-10-15.** Bus init cannot all move before the scheduler (`PeripheralManager.openSpi`
  and a Java `I2cDevice` initialise a bus from a task), so the claim became structure instead:
  `hal/rp/init_gate.rs` gives each bus an `InitGate`, a three-state atomic whose `claim` is one load
  and one store under `cortex_m::interrupt::free` (every bus initialiser runs on core 0, the
  section `periph::enable_irq` uses). Exactly one caller gets `Claim::Ours` and runs the init; one
  that arrives during it gets `Claim::Busy` and waits on an RTOS delay for `finish`; a later one
  gets `Claim::Done`. `is_initialised` and the check-then-store are gone from both buses, and the
  cells' SAFETY comments rest on the gate, not on the callers not overlapping.
  `JVM_TASK` and `PDB_TASK` are now set by `boot_tasks` with the handle `spawn` returns, before
  `start_scheduler`, not by each task as its first statement: no task has run, so no reader can
  exist. Each setter asserts `scheduler_state() == NotStarted` (a plain `assert!`, since
  `debug_assertions` is off in firmware), so the contract is checked, not described. Build-checked
  on every clippy leg and sim-smoked; not run on a board.

## Framework (picodroid-core)

### UR-3

**`ViewGroup.addView` can make an LVGL parent cycle.**

- **Where:** `crates/picodroid-core/src/graphics/lvgl/view_ops.rs` :184–191, which has an OPEN note.
- **What:** `lv_obj_set_parent(child, parent)` doesn't check that `parent` is outside `child`'s
  subtree, and `ViewGroup.addView` on the Java side doesn't either. Android throws "The specified
  child already has a parent" earlier, but a view added into its own descendant reaches LVGL.
  LVGL then has a cycle:
  - layout, draw and delete walk it forever, or recurse until the stack overflows;
  - `lv_obj_delete` on the cycle frees an object still referenced as a parent.
- **Fix:** before the call, walk `parent`'s ancestors with `lv_obj_get_parent` and refuse
  (`Reparent::Cycle`) if one is `child`. Throw `IllegalStateException` on the Java side, as Android
  does for an already-parented child. The walk is bounded by tree depth.
- **Verify:** add a `qa_ui` probe that adds a LinearLayout into its own child and expects the
  exception.
- **Fixed 2026-10-15**, both halves. `view_ops::set_parent` walks from `parent` to its screen
  with `lv_obj_get_parent` and returns `Reparent::Cycle` when the walk meets `child`, before the
  `lv_obj_set_parent` call; `view_group::add_view` turns that into an `IllegalStateException`.
  `ViewGroup.attach` on the Java side walks its own `mParent` chain first and throws the same
  exception before any native call, so the native check is the backstop. Two `qa_ui` probes in
  `tree()` (a layout into its own child, a layout into itself) pass with either check alone: the
  suite was run once with the Java check disabled to prove the native one.

### UR-4

**`window::sync` deletes the old compat window behind the keyboard's back.**

- **Where:** `crates/picodroid-core/src/graphics/lvgl/window.rs` :165–170 (`unsafe { lv_obj_delete(old) }`);
  the keyboard state is in `widgets/keyboard.rs`, `bound_ta` at :109 and :188.
- **What:** switching window design deletes the old window object directly. Other paths go through
  the view delete path, which unbinds the system keyboard and cancels animations; this one doesn't.
  - If an EditText inside the old window is bound to the system keyboard, `bound_ta` keeps the
    freed textarea's address.
  - The keyboard's later defocus and READY handling then call LVGL on freed memory (`bound_ta()`
    turns the stale address back into an `Obj`).
  - The same goes for a running animation on any object in the old window.
- **When it fires:** an app that changes its compat-window design while the keyboard is up. That's
  rare, but it is a use-after-free.
- **Fix:** before the delete, call the keyboard's unbind (`hide_system` plus clearing `bound_ta`)
  and `animations::cancel_subtree(old)`, or route the delete through the same helper `view_ops::delete`
  uses. A `LV_EVENT_DELETE` hook on the bound textarea that clears `bound_ta` would cover every
  path at once.
- **Fixed 2026-10-15** (the helper). `window::sync` deletes the old window through
  `view_ops::delete_obj`, the path every other framework delete takes, so `unbind_if_deleting`
  clears `bound_ta` and hides the keyboard, and `animations::cancel_subtree` retires the subtree's
  slots, before LVGL frees the tree. The `bound_ta()` SAFETY comment no longer carries the
  exception. On today's one caller, the app reload in `boot.rs`, the keyboard state is reset
  before `set_design(None)` runs, so the invariant used to hold by call order; it now holds by
  structure. Not reproduced; build-checked and sim-smoked only. The bare `lv_obj_delete` calls
  left in `toast.rs`, `snackbar.rs` and `alert_dialog.rs` are on framework widgets no EditText can
  be bound inside, and were not changed.

### UR-11

**An app reload leaks the system keyboard.**

- **Where:** `crates/picodroid-core/src/graphics/lvgl/widgets/keyboard.rs`, `reset_keyboard_state`
  (:476). Compare with the `State.system` doc at :92–95.
- **What:** the system keyboard lives on LVGL's top layer, which `handle_table::reset` doesn't tear
  down. The reset comment assumes the screen tree took it with it, so it forgets the `Obj` without
  deleting it. The next show creates a new keyboard, and the old one stays on the top layer.
- **When it fires:** every pdb app reload that showed the keyboard leaks one keyboard object from
  the LVGL pool, and its event callbacks stay attached.
- **Fix:** delete the cached keyboard in the reset (it is still live), or keep it across reloads
  and re-bind it.
- **Verify:** `lv_mem_monitor` free bytes before and after ten reloads that each open the keyboard.

### UR-12

Simulator only.

**`Heap4::free` is safe and public, but trusts its offset's alignment.**

- **Where:** `crates/heap4/src/lib.rs` :230. This is the simulator's port of FreeRTOS `heap_4`,
  used by the sim's capped allocator.
- **What:** it checks that the offset is inside the arena and that the header looks allocated. An
  offset that is in range but misaligned, or points into the middle of a block, makes it read a
  "header" out of payload bytes: an unaligned read, and a free-list insert at a bogus address.
- **Fix:** make `free` an `unsafe fn` whose contract is "an offset `malloc` returned and not yet
  freed". That moves the obligation to the allocator, the one real caller, and costs about 11 test
  blocks.

### UR-13

Simulator only.

**Two Java threads on one simulator socket hold two `&mut SimSocket`.**

- **Where:** `crates/picodroid-core/src/hal/sim/net.rs`, `deref_socket` (:126). The doc says so.
- **What:** a reader blocked in `tcp_recv` releases the JVM run lock while holding a
  `&mut SimSocket`. A second thread can then send on the same socket and take another one.
- **Fix:** hand out `&SimSocket` with the mutable parts behind a per-socket lock (`Arc` plus a
  lock that can't priority-deadlock; see round-1 §4.1), or split the stream with `try_clone` so the
  read and write halves are separate objects.

### UR-14

Parity-metrics builds only.

**`slowest_native` can build a string from a mismatched pointer and length.**

- **Where:** `crates/jvm/src/parity.rs`, `count_native` (:128) and `slowest_native` (:155).
- **What:** `count_native` stores the slowest call's name as four separate atomics (two pointers,
  two lengths). A reader racing a writer can pair one name's pointer with another's length and read
  out of bounds. `count_native` also accepts `&str` of any lifetime but republishes it as
  `&'static str`.
- **Fix:** store an index into a table of names, or a single `&'static str` behind a seqlock. Or
  take `&'static str` in `count_native`, which every caller can provide.

### UR-15

**`pd_install::install` is safe and public over a static page buffer.**

- **Where:** `crates/pd-install/src/orchestrator.rs`, `PAGE_BUF` (:33), `install` (:191), and the
  use at :500–502.
- **What:** `install` borrows the static `PAGE_BUF` mutably. It is sound only because the debug-bridge
  task is the one caller. Two concurrent installs, for example a future second install path, would
  alias it.
- **Fix:** take the buffer as a `&mut [u8; 256]` argument (the pdb task owns one), or make
  `install` `unsafe` with the one-caller contract stated.

## Not bugs, recorded for context

- Round-1 §6's `ShadowCell` finding (`executors/main_queue.rs`, a `Cell` written from two tasks)
  no longer holds: every push, remove and clear opens an `AtomicSection`, and the GC root walk runs
  inside the collector's. It was checked again for this doc.
- `pd-rtos` hands out raw `usize` handles through safe methods. `glue.rs` and the simulator's
  `rtos.rs`/`net.rs` turn them back into references, and each cast is documented. A wrong handle is
  UB, but only framework code makes handles. Typed handles in `pd-rtos` would move this to the type
  system; that is a design change, not a bug fix.
- `MemRegion::restore` (pd-install) is a safe mutator over a region that live `&'static [u8]`
  borrows point into. On the device the region is flash and the contract is enforced by the install
  flow; on the host it is a test fixture.
