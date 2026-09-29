# Completed: Open Follow-ups — QA round 2026-09-13

Items closed out of [qa-2026-09-13-followups.md](../qa-2026-09-13-followups.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 1. `qa_life` stalls on the Pico 2 W slot — **found and fixed 2026-09-15** (P1)

**It was never the main-executor post.** The UI task was sitting inside an SPI native, in
`finish_isr_xfer!` (`platforms/rp/src/hal/rp/spi/mod.rs`), waiting out a 5,000 ms cap for a
transfer-complete interrupt that could no longer come — silently, because the timeout was
swallowed with no log line. Every 5 s the app lost was one of those. A run that hit enough of
them overran the row's 60 s window and was recorded as a stall; a run that hit few passed.
That is the whole of "one run in two".

**The mechanism.** The ISR ends an interrupt-driven transfer by counting the bytes it has
*received* (`rx_idx >= len`). On this slot an XPT2046 poll — 30 bytes at 2 MHz, full duplex —
regularly ends with `rx_idx == len - 1`, the shifter idle, the RX FIFO empty, and neither the
overrun nor the receive-timeout status asserted:

```text
spiprobe: spi1 timeout op=1 len=30 tx_idx=30 rx_idx=29 sr=0x3 ris=0x8
```

The completion test can then never be satisfied, and no further interrupt can arrive either:
the receive timeout only asserts on a FIFO that is *not* empty. So the waiter blocks for the
full cap. Whether the byte was dropped by the eight-deep FIFO or its receive-timeout was
cleared by step 5 of the ISR body as it asserted is **not settled** — the fix does not depend
on which.

**Why only this slot.** Write-only transfers go through DMA; the ISR path only ever sees
full-duplex work. The single full-duplex user on these boards is the XPT2046, which shares the
display's SPI bus. `pico_touch_kit` reads a GT911 over I²C (`touch_private_bus`) and the Enviro
pack has no touch at all, so neither board can reach this path — which is why the sim, the
touch kit and the enviro firmware never stalled, and why the two failing firmwares are exactly
`testbench_rp2350` and `testbench_rp2350w`. `testbench_rp2040` carries the same wiring and
**does** hit it — its `qa_life` run logs the same short read — but passes anyway, so there the
defect has been costing whole seconds silently without ever failing that row. Anything on that
board that has been intermittently missing a deadline is worth re-reading in this light: the
`helloworld:pdb-install[shrink]` reboot that "missed the 20 s PING window", intermittent since
2026-09-12, is the obvious candidate.

**What the JVM run lock changed.** Nothing, either way — it neither caused this nor fixed it
(reproduced first try on `0a785461`, run lock included). It does widen the blast radius: the UI
task holds the run lock across the native, so since `d1a09765` every Java thread blocks with it.
That is why a child's `SystemClock.sleep(80)` measures 5046 ms. Before the run lock only the UI
loop froze — which is exactly the "post is queued and the loop never runs it" the first report
described.

**Corrections to the earlier notes.** The suspected cause — "the second core carrying the cyw43
task" — cannot apply: `testbench_rp2350` has `has_network = false`, so that image has no cyw43
task at all and core 1 carries nothing but the flash parker. And a lost wake was never needed:
`pdb sysmon` during a stall shows the kernel tick advancing normally (146,638 → 151,795 ticks
over 5 s), every Java task `Blocked`, both idle tasks at ~100 %, and `Tmr Svc` alive — the tick
source was firing and being coalesced away (`calls=256 posted=3 coalesced=253`) because the UI
loop was not draining the queue, not because the tick had stopped.

**The fix** (`platforms/rp/src/hal/rp/spi/mod.rs`): the wait also accepts the controller's own
account of being finished — everything queued shifted out, shifter idle, RX FIFO empty — and
ends the transfer there. What is lost is one sample rather than five seconds. It is reported
once per boot and counted in `SHORT_READS` thereafter, so a board where this is the steady state
says so instead of freezing. The 5 s cap stays for the case it was meant for: a device holding
the bus, which is a fault to report rather than to wait out.

**Verified on the bench.** `qa_life` on the slot that stalled it: 6 runs, 6 passes, each run
~80 s against the ~120 s the stalling runs took (before the fix the same loop stalled on its
second run, and `hil-run.sh --app qa_life --board testbench_rp2350` failed first try on
`0a785461`). `qa_life` on `pico_touch_kit`: 74/74, no short-read line at all — that
board never takes this path. `qa_life` on `testbench_rp2040`: 74/74 *with* a short-read line,
which is how we know that board has been paying for this too. The boot line now says once what is happening:

```text
spi1: transfer finished with 29 of 30 bytes read back; completing it from the controller's
state (further occurrences are silent)
```

`platforms/rp/src/hal/rp/spi/xfer.rs::controller_finished` carries the decision as a pure
function with five host tests, so the rule is checked by `./scripts/test.sh` rather than only by
a board. The `spin_guard` ledger rejected the first draft of the semaphore drain, which is the
guard doing its job.

**Repro kept.** `examples/postloop` — 60 lines, one fresh child `Thread` per hop that sleeps
80 ms and posts the next hop to `Executors.mainExecutor()`, with a prefs commit every tenth hop,
logging each stage's cost. On this slot before the fix: `slept=5046`, `up=5001`, `slept=10046`,
roughly one hop in four. On `pico_touch_kit`, every hop `slept=79/80`. After the fix, 30+ hops
with no stage over 3 ms. It is not in any run matrix — it is a bench instrument.

**Left open.** Which of the two candidate causes drops the byte (an ISR-side fix would stop the
sample being lost at all, not just stop the freeze); why `testbench_rp2040` does not show it.
Settled 2026-09-15: the WP7 tick-timebase failures on
this slot were this bug for `animdemo` (plain `main` hit the same timeout here, `spi1 ...
rx_idx=28`) and a bug of WP7's own for `alarmdemo` (its RTC-alarm gate; `9d090232`) — see
`scheduling-audit-handover-2026-09.md` §2 WP7. WP7 is re-landed.

## 2. Devices ran the legacy handle cast — **flipped 2026-09-15** (P1)

Every bench board cast `lv_obj_t*` to a 32-bit handle, so the sim's stale-handle answers
(`Reparent::StaleChild`, `is_live`, the sanitizer) did not exist there: a stale widget handle
was a use-after-free. F10 moved the released state into Java (`View.release()`, refused by
`addView`, `IllegalStateException` on a released receiver) so the app-driven cases were
covered, but a second `AlertDialog.dismiss()` after a keypad BACK was still a dangling pointer
on a device. The generation-tagged table was staged behind the default-off `handle-table-32`
feature (`designs/handle-table-invalidation.md`).

**2026-09-14.** `qa_ui` passes with the table on both RP2350 boards in both shrink modes
(`pico_touch_kit` 146/146 ×2, `testbench_rp2350` on the Pico 2 W slot 141/141 ×2). Cost
against the size baseline: +2,400 B flash / +1,032 B RAM on `testbench_rp2040`, +3,352 B
flash / +1,024 B RAM on `testbench_rp2350` (the table itself is the kilobyte of `.bss`).

**2026-09-15 — closed.** The table is the default for every target; the cast is gone from every
build and survives one release behind the opt-out `legacy-handle-cast` feature, which is what
`pre-commit --full` now lints (one thumbv6m leg — the table arm needs no staged leg any more,
since every board build compiles it).

*Why the feature had to be inverted rather than added to `default`:* firmware builds run
`--no-default-features --features board-X` (`lib.sh::build_firmware`), so a `default = [...]`
entry would never have reached a board. Cargo features are additive, so "on unless you say
otherwise" is spelled as an opt-out feature.

**The soak.** `picoenvmon --release --shrink` on the `pico_enviro_mon_w` slot (the Pico 2 W
with the Enviro+ pack; sensors, WiFi, NTP and the weather fetch all live), driven over PDB by
`pdb input keyevent` through the 4-button hub: 60 cycles, each opening one of Live / History /
Network / Settings, working it and coming back, with the History visit opening a sample
`AlertDialog` twice — once dismissed by keypad BACK, once by its OK button, which is exactly
the double-dismiss case that used to dangle. Seven `pdb install` reloads — one to boot the app,
then one per ten cycles — so `handle_table::reset()` ran seven times with the screen's pinned
slot carried across. Result: **no fault, no panic, no stale-handle symptom**; free heap flat
between 109.2 and 112.8 KB across every reload (low-water 98.6 KB), which is where a leaking
delete hook or a leaking slot would have shown. The only `[ERROR]` line in the whole run is one
`Thread.start: ... left the interpreter: Interrupted` per reboot — a teardown message that
predates this work (it is in the 2026-09-09 and 2026-09-12 HIL logs). A second 120-cycle
pass with the same driver was cut at cycle 11 (clean, heap 108.4-111.0 KB) to hand the slot
to a session queued behind it. Driver and logs: `/tmp` scratch of the flip session; the
recipe is four lines of `pdb input keyevent` and worth rewriting rather than restoring.

One driver trap worth keeping: Settings' Save button **finishes its own activity**, so a
cycle that presses it must not also press BACK — the BACK reaches the hub, finishes
`HomeActivity` and drops the board into the launcher, which ends the picoenvmon soak while
the log still looks busy. Have the driver watch the RTT log for `Launcher: creating` and
re-install rather than trusting the key counts.

**The RP2040's first run of the table.** Every earlier hardware check was an RP2350 — the
RP2040 only ever *built* with the feature (the old pre-commit link gate), so the flip changed
what that board executes with no run behind it. Four widget-heavy rows on
`testbench_rp2040`, no-shrink, after the flip: `bugbash_ui`, `callbacktest`, `dialogdemo`
and `animdemo`, all PASS (`build/hil/logs/testbench_rp2040/2026-09-15_23h*`). `dialogdemo` is
the pointed one — it builds and dismisses three dialogs — and `animdemo` exercises the
animation engine whose hardware-only hang is the incident HAL-05 was written from.

**Cost, measured on this tree** (helloworld, release, no-shrink; the same tree built with
`PICODROID_EXTRA_FEATURES=legacy-handle-cast` is the control, so nothing else on `main` is
folded into the figure):

| board | cast | table | table costs |
|---|---|---|---|
| `testbench_rp2040` | 822,444 B flash / 244,400 B RAM | 824,892 / 245,432 | **+2,448 B flash, +1,032 B RAM** |
| `testbench_rp2350` | 998,152 B flash / 514,660 B RAM | 1,001,208 / 515,684 | **+3,056 B flash, +1,024 B RAM** |

The accepted ratchet moves further than that — +3,177 B on the RP2040 and +4,465 B on the
RP2350 — because the control run also shows 729 B (rp2040) and 1,409 B (rp2350) of growth that
landed after the last accept (`d1a09765`) and was waiting for tonight's nightly to find. RAM
matched the baseline exactly in cast mode, so all of the RAM growth is the table.

## 3. Unchecked allocations left in native paths (P2 — a board reset each) — **closed 2026-09-16**

J23–J26 made the formatter, file streams, frames and interning fallible; three infallible
sites remained, each a reset instead of an `OutOfMemoryError`. All three are closed now (the
RP2040's 10 KB, the LittleFS cache and the touch kit's 7680 bytes), plus one found on the way
(`ChunkedSlots::push`):

- **Touch kit, 7680 bytes** on the path a background-pool worker takes into Java under a full
  heap (`qa_thr` on `pico_touch_kit` without the conf restriction). It is not the frame or the
  task stack — **named and fixed 2026-09-16: the lambda registry**
  (`object_heap/lambda.rs`, `ObjectHeap::register_lambda`). Every `invokedynamic` recorded its
  proxy with a plain `Vec::push` into `lambda_proxies`; at 60 bytes an entry the doubling from
  64 to 128 registered lambdas is exactly one contiguous **7,680-byte** request. `qa_thr`'s
  `frameworkExecutors` section posts 64 `execute(() -> …)` lambdas in a burst on a heap the
  earlier sections had already filled (every `Thread.start` there fails with a clean
  `OutOfMemoryError("unable to create native thread")`), and the 65th lambda tipped it over:
  `memory allocation of 7680 bytes failed` → `panic_probe`'s `udf`. Not a background-pool
  path at all, then — the pool workers were merely what still ran Java at that point.
  `register_lambda` (and `iter_register`, the same shape) now go through `reserve_fallible`
  and answer `Exhausted`; `invokedynamic` turns that into the allocation-failure signal, the
  catchable `OutOfMemoryError`, and the captures copy on the same path reserves fallibly too.
  Two `with_budget` tests in `object_heap/mod.rs` pin both registries.

  How it was named: the RP2040 recipe (the `#[global_allocator]` wrapper that stashes thumb
  return addresses for any request ≥ 4 KB, no logging inside `alloc`) — with one RP2350
  difference. probe-rs catches the `udf` as an exception and exits before the `HardFault`
  handler prints anything, so the stash was read out of the halted board instead:
  `arm-none-eabi-nm` for the statics' addresses, then
  `probe-rs read --chip RP235x --probe <touch kit's probe> b32 <addr> <n>`, and
  `arm-none-eabi-addr2line -f -C -i` on the words gave `finish_grow` ← `Vec::push` ←
  `register_lambda` ← `op_invokedynamic` on the first try. No heap census needed.
- **RP2040, 10 KB** in `qa_ui`'s focus section once the containers section has exhausted the
  heap (`qa_ui` is restricted to the RP2350 boards in `hil-tests.conf` for this reason) —
  **done 2026-09-15**, see below.
  **2026-09-15, measured on the board** (`PICODROID_EXTRA_FEATURES=mem-diag flash.sh -b
  testbench_rp2040 -a qa_ui`): the size is exact — `memory allocation of 10240 bytes failed`
  — and the reset is that panic reaching `panic_probe`'s `udf`, not memory corruption: the
  stacked frame reads `pc=__udf`. `containers` raises a clean `OutOfMemoryError` first, so
  the heap is genuinely full by then and any large infallible request would do it.
  Markers between the Java statements put the failing allocation between
  `setFocusable(true)` and the `new int[1]` that follows — on a board with no keypad group
  `setFocusable` is nearly a no-op, so it is the string work in `check()` that tips it over,
  not the focus natives.
  **Named and fixed 2026-09-15: the interpreter's method/field resolution caches**
  (`interpreter/helpers.rs`, `interpreter/ops_fields.rs`). `MethodCacheEntry` is
  `(*const u8, *const u8, *const u8, usize, usize)` = 20 bytes on a 32-bit target, and the
  cache `Vec` doubling from 256 to **512 entries is exactly 10,240 bytes** — one contiguous
  request, made by a plain `Vec::push`, on a heap `containers` had already emptied.
  All five cache pushes go through `helpers::cache_push`, which `try_reserve(1)`s and
  simply declines to memoise if the heap says no: these are caches, not state, so the cost
  of a refusal is one re-resolve. On the board `qa_ui` now runs through the focus section
  (`requestFocus declined (touch board)`) instead of resetting.

  How it was found, since neither obvious tool worked: the sim cannot stand in (it exhausts
  the 160 KB model back in `radio`, and its 64-bit entry is 40 bytes, so the size does not
  even match), `probe-rs gdb` does not work on this board, and Cortex-M0+ cannot be unwound
  through the exception. What worked was a temporary `#[global_allocator]` wrapper that, for
  any request ≥ 8 KB, scanned 200 words above `sp` for values that look like thumb return
  addresses into the XIP text region and stashed them in a `static` — **with no logging**,
  because a `defmt` call inside the allocator takes a critical section under the FreeRTOS
  heap lock and wedges the board before RTT attaches. The `HardFault` handler printed the
  stash afterwards, and `arm-none-eabi-addr2line -i` turned it into
  `finish_grow` ← `Vec::push` ← `find_method_cached` ← `op_invoke`.
- **`ChunkedSlots::push`, one `CHUNK_SIZE` chunk** — **done 2026-09-15.** `ArrayHeap::alloc`
  takes care to answer `None` from both of its arena reservations, then placed the slot with
  `push`, which allocates a fresh chunk through an infallible `Vec::with_capacity` — so the
  last step of a carefully fallible allocation was the one that could abort the board, once
  every 64 arrays. It now uses the `try_push` the object and string stores already used, and
  rolls the arena back when the slot cannot be placed. That was `push`'s last caller, so it
  is `#[cfg(test)]` now and firmware cannot reach the infallible path at all.
- **LittleFS file cache, 4 KB per open** — **done** (`2cfb0120`): the allocation was
  `vec![0u8; cache_size]` in `littlefs-rust`'s `File::open`, not in `hal_impl.rs`; the crate
  is vendored under `third_party/littlefs-rust` with the cache reserved through
  `try_reserve`, the HAL answers -2 for `NoMemory` on `read_at`/`write_at`, and the stream
  and `createNewFile` natives throw `OutOfMemoryError` for it.

The rule the round established: the sim's device heap model is the RP2350's; the RP2040
(160 KB) and the touch kit (LVGL draw buffers) are tighter, so every infallible allocation in a
native path is a reset waiting for a big enough app. The source scan that keeps the list from
growing is **in** (`f5b78132`, `native_handler/alloc_scan.rs`): it counts the whole-buffer
allocation shapes per file under `crates/jvm/src/native` and `native_handler/` against a
committed baseline (42 sites in seven files) and fails on growth or on an unaccepted
improvement. Both device findings above are named and fixed: the RP2040's 10 KB on
2026-09-15, the touch kit's 7680 bytes on 2026-09-16. Neither lived under the scanned trees
(`interpreter/`, `object_heap/`), which is the scan's known gap.

## 4. RP2040 negative-fraction casts floor framework-wide — **done 2026-09-14** (P2)

J22 fixed the JVM's `f2i`/`f2l`/`d2i`/`d2l` and the box accessors, but the cause is the HAL:
rp2040-hal maps `__aeabi_f2iz`/`f2lz`/`d2iz`/`d2lz` to the bootrom's `float_to_int` family,
which rounds toward −∞, so every `as i32` of a negative fraction in picodroid-core and
platforms/rp (animation offsets, sensor scaling, layout maths) floors on that board and
truncates everywhere else.

**Options.** (a) Truncating `__aeabi_*2iz`/`2lz` symbols of our own in place of the bootrom
mapping — needs the HAL to stop defining them; (b) rp2040-hal's `disable-intrinsics`, which
also drops the ROM float speed-ups (measure `benchmark` on `testbench_rp2040` first); (c) a
crate-wide `fconv` helper plus a source scan forbidding bare `as i32` on floats. (a) or (c)
is the durable one; decide with the benchmark numbers in hand.

**2026-09-14, measured and decided.** (b) is out: `disable-intrinsics` also drops the SIO
hardware-divider intrinsics, and `benchmark` on `testbench_rp2040` (no-shrink, one run each)
went from 344,617 ms to 383,736 ms TOTAL (+11 %), with `double_arithmetic` 23,916 → 40,178 ms
(+68 %) and `interface_dispatch` +16 % — past the 10 % budget even allowing the ±40 %
placement noise a single section can show. (a) needs no symbol override after all: the HAL is
vendored (`third_party/rp2040-hal`, `[patch.crates-io]`, the same arrangement as the two
littlefs crates) with the four conversion intrinsics fixed at the source — a negative input
goes through the ROM negated (`floor(-f) = trunc(f)`), the type's minimum is answered
directly. That corrects every `as i32` in Rust and every `(int)` cast in LVGL's C alike, since
both link the one `__aeabi_f2iz`. `platforms/rp/src/main.rs` checks `black_box(-1.5) as i32`
at boot on the RP2040 and logs `[float] f2i floors …` as an error if a HAL upgrade ever drops
the patch. The fixed image's `benchmark` numbers are recorded in the session notes below.

## 5. A foreign LittleFS geometry should format, not fail to mount — **done** (P3)

When the Pico 2 W slot booted a touch-kit image (see 6), its LittleFS region was left
formatted for that board's geometry; the testbench firmware then boots with
`[fs] init failed: invalid parameter` and every open fails — `qa_life`'s prefs and a whole
`qa_store` run — until `probe-rs erase --chip RP235x`. The runtime could treat a geometry
mismatch like a missing superblock and format the volume (with a log line), as it does for a
blank chip. Cheap; the `bootcount` example plus a power cycle verifies it.

**Done** (`5dffb2ab`): `fs/volume.rs` formats on `Invalid` as on `Corrupt`, with a
`[fs] mount failed: foreign superblock (geometry or version); formatting the volume` warn;
host tests cover a clean remount (keeps files) and a foreign block count (formats).

## 6. Harness: same-family boards must not run `hil-run.sh` in parallel from one worktree — **done** (P3)

Two `hil-run.sh` invocations for boards of one MCU family share
`target/<triple>/release/picodroid`, so one board can be flashed with the other's image (the
Pico 2 W once booted a touch-kit build, probing PSRAM and a GT911 that are not there).
`hil-fleet.sh` avoids it with a target directory per slot; a bare `hil-run.sh` does not.
Give `hil-run.sh` the same per-slot `CARGO_TARGET_DIR` (or make it refuse to start while a
same-family run holds the ELF), so the nightly's safety extends to one-off runs. The RP2040
(thumbv6m) is safe alongside either RP2350 board.

**Done** (`fb0d1094`): with a fleet config a bare run defaults to the nightly's
`build/hil/<slot>/{target,apks}`; the item-2 runs above went on both RP2350 slots at once.

## 7. `IllegalFormat*` exception names — **done** (P3)

The formatter now distinguishes the cases (J19c) but throws the family's base class,
`IllegalFormatException`, for a precision on an integral conversion. Java throws
`IllegalFormatPrecisionException`; giving the family its class names is a registry entry
each (`project_class_name_to_static`, `PICODROID_NATIVE_CLASSES`), and the `qa_coll`
divergence log line is the test.

**Done** (`e5d5b88c`): `IllegalFormatPrecisionException`, `MissingFormatArgumentException`,
`IllegalFormatConversionException` and `UnknownFormatConversionException`, each extending
`IllegalFormatException`; `qa_coll`'s four format checks catch the specific class.
