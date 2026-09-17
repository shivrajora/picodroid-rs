# Open Follow-ups — QA round 2026-09-13

Everything the 2026-09-13 QA round left open (`qa-2026-09-13.md`: seven `qa_*` apps, forty
fixes, three boards). Ordered by risk. Each entry says what was seen, what is already ruled
out, and the next concrete step, so it can be picked up cold. Status as of 2026-09-14
(branch `qa-followups-2026-09-14`): items 3 (in part), 4, 5, 6, 7 and 8 are landed, each as
one commit; item 2 is soaked on the bench and awaits the picoenvmon soak. **Item 1 — the last
open P1 — was found and fixed on 2026-09-15**; it was an SPI transfer that never signalled
completion and was waited out silently for five seconds at a time, not a lost executor post.
Section 9 records the regression coverage every fix of the round now has.

**Status 2026-09-16:** items 1, 2, 4, 5, 6 and 7 are closed and item 8 is documentation. Item 3
has one site left, the touch kit's 7,680 B, still unnamed. Smaller residues, none of them
blocking: which cause drops the SPI byte in item 1, and why the RP2040 does not show it; an
automated check for `SharedPreferences.commit()` at the quota (§10); `qa_thr` restricted off the
W board for want of heap (§11); and two unreproduced one-offs to watch in the nightly — the touch
kit's empty RTT capture and the RP2040's `pdb-install[shrink]` reboot missing its PING window (§11).

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

## 3. Unchecked allocations left in native paths (P2 — a board reset each)

J23–J26 made the formatter, file streams, frames and interning fallible; three infallible
sites remained, each a reset instead of an `OutOfMemoryError`. Two are closed now (the
RP2040's 10 KB and the LittleFS cache), plus one found on the way
(`ChunkedSlots::push`); the touch kit's 7680 bytes is the one still open:

- **Touch kit, 7680 bytes** on the path a background-pool worker takes into Java under a full
  heap (`qa_thr` on `pico_touch_kit` without the conf restriction). It is not the frame or the
  task stack. A device heap census (`docs/memory-diagnostics.md`, `--mem-diag` on the sim
  first) is the way to name it.
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
improvement. Of the two device findings above, the RP2040's 10 KB is named and fixed
(2026-09-15); the touch kit's 7680 bytes is still open and still needs the heap census to name.

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

## 8. Expectations to document, not fix

- The RP2040's 160 KB heap holds far fewer boxed entries than the sim's device model; an app
  that grows a collection past a few hundred boxes hits `OutOfMemoryError` there first. The
  QA apps are sized for it; the sizing rule is now on the website's limits page
  (`reference/limits.md`, "Boxed collection entries"), whose per-board heap table was also
  brought back in line with the MCU TOMLs (160 / 408 / 344 KB, not 128 / 416 / 408).
- Divergences the round kept on purpose (ASCII strings, subnormal minima in shortest form,
  no element class on reference arrays, `removeView`/`dismiss` freeing the widget,
  `equals`-only hash collections) are listed in `qa-2026-09-13.md` and logged by the apps.

## 9. Housekeeping for whoever picks these up

- The apps live in `examples/qa_{lang,oop,coll,store,thr,life,ui}`; each prints
  `=== ALL PASSED ===` or the failing check. Rows in `scripts/hil-tests.conf` restrict
  `qa_thr` to the RP2350 testbench boards and `qa_coll`/`qa_store`/`qa_ui` to `rp2350,rp2350b`.
- Per-board reruns: `./scripts/hil-run.sh --app qa_life --board testbench_rp2350 --no-email
  --no-pull`; both shrink modes run. Release the session's lease before a detached run.
- The one-commit-per-bug driver (snapshot tag + marker strips) is described in the report;
  reuse it for the next round rather than committing fixes in a batch.

## 10. Session notes, 2026-09-14

Branch `qa-followups-2026-09-14`; one commit per item, the size baseline accepted in each.

**`benchmark` on `testbench_rp2040`** (no-shrink, one run each, ms; the ±40 % per-section
placement noise applies, TOTAL is the number to read):

| section | ROM intrinsics (unpatched HAL) | `disable-intrinsics` | vendored HAL, truncating |
|---|---|---|---|
| int_arithmetic | 32,186 | 33,512 | 31,039 |
| long_arithmetic | 12,850 | 14,047 | 12,579 |
| float_arithmetic | 17,128 | 19,417 | 17,189 |
| double_arithmetic | 23,916 | 40,178 | 23,495 |
| method_dispatch | 63,554 | 66,436 | 56,507 |
| interface_dispatch | 41,611 | 48,448 | 40,569 |
| object_allocation | 47,439 | 51,391 | 48,464 |
| array_operations | 47,878 | 50,260 | 48,172 |
| string_operations | 44,309 | 46,502 | 43,279 |
| control_flow | 13,741 | 13,541 | 13,149 |
| TOTAL | 344,617 | 383,736 | 334,447 |

The boot check was seen to fire on the unpatched HAL (`[ERROR] [float] f2i floors instead of
truncating …` in `helloworld`'s RTT) and to stay silent on the vendored one.

**`qa_life` on the Pico 2 W slot (item 1).** The reproduction loop: `flash.sh --board
testbench_rp2350 --app qa_life -r` in the background, poll its RTT log every 2 s, call it a
stall when the log has `QaLife` lines, no `=== ALL PASSED ===`/`=== FAILED`, and has not grown
for 20 s, then `pdb sysmon` twice 5 s apart (pdb goes over USB CDC, so it answers while the
probe streams). Between runs the previous `probe-rs run` has to be killed by pid — it outlives
`flash.sh` in a session of its own, and a `kill` of the launcher's process group or session
leaves it holding the USB interface ("interface is busy", the next run's "Failed to open
probe"). **Outcome: no stall in eight runs** — four through that loop (74/74 each) and four
through `hil-run.sh --app qa_life --board testbench_rp2350` (both shrink modes, twice, with the
`sysmon` watcher armed on the live row logs). Whatever stalled one run in two on 2026-09-13
did not show on this branch's firmware; the 4 AM fleet run on the same slot is the tripwire,
and the watcher recipe above is how to catch the task state when it does.

**`qa_ui` under `handle-table-32`** ran on both RP2350 slots at once from this checkout — the
first time that was safe — 4/4 passes; size cost in item 2.

**Left for the next session, in order:** the picoenvmon soak under `handle-table-32` and the
default flip (item 2); the two device heap censuses (item 3); item 1 if it reproduces in the
nightly. (It did, on 2026-09-15, and item 1 is now closed — see that item.)

## 9. Regression coverage for the round's fixes (2026-09-14)

The round landed forty-odd fixes; nine shipped with a host test, and the rest were guarded only
by the `qa_*` apps on the nightly HIL run — hardware, once a day, three boards. This pass gave
every fix a check that runs before a push. Three kinds, by what the fix's code is reachable
from:

**Host unit tests** (`./scripts/test.sh`, both shrink modes) for everything a `cargo test` can
call: the formatter's rounding, widths, precision rejection and exception classes; `String`'s
`compareTo` and empty-target `replace`; the boxed accessors' JLS conversions and the four
`parse*(null)` throws; `Random.nextInt`'s bound; the collections' key equality, buffer release
and user-`equals` path; `Enum.valueOf`; `StringBuilder.append(CharSequence)`; `Arrays` and
`System.arraycopy` on a null array; the file streams' 256-byte chunk loop. Each was checked
against its own regression — reverted or mutated the fix, confirmed the test fails, restored.

**A capped allocator** (`jvm/src/test_alloc.rs`) for the out-of-memory fixes, which are
untestable without an allocation that fails: a `#[cfg(test)]` global allocator with a
per-thread byte budget, charged on allocation and refunded on free, so a test can hand the
interpreter a fixed heap. It covers J23–J26's fallible frames and interning, the builtin arm's
collect-and-retry, `new`'s rewind, and the catchable `OutOfMemoryError`.

**Sim lanes and shape guards** for the fixes whose code a host test cannot reach — the
`graphics` tree and `native_handler` are `cfg(not(test))`, and some of the round's fixes are in
the SDK's own Java. The seven `qa_*` apps now run in the sim as the `qa` lane of
`./scripts/pre-commit --full`, which is the behavioural check: reverting the SeekBar fix fails
`qa_ui` there in 100 seconds. They sit on the `host` lane behind the langsuites, not on a lane
of their own — two sim lanes at once race on the shared Gradle stage and the framework map the
firmware embeds, and the loser boots to `FrameworkVersionMismatch` (which is what the first
parallel run did, and why the three langsuites were already serial there). Alongside them,
`picodroid-core/src/qa_shape_guards.rs` text-scans for the shape each unreachable fix put in
place — the UI-task ledger before `Application.onCreate`, the logged Runnable failure, the
checked `enqueue_op` results, the released-receiver refusal, the click-listener throw — so a
refactor that drops one fails in seconds instead of in the next QA round.

Three notes from the pass:

**A new infallible allocation, fixed.** `ObjectHeap::alloc` grew its slot table with an
infallible `ChunkedSlots::push`, so the most Java-reachable allocation there is — every `new`,
every box — aborted the firmware once the heap was full, which on a device is a board reset.
Found by the first out-of-memory test written against a fixed heap. `place_in_slot` and
`intern_class` are now fallible and roll the field span back on refusal; the callers already
handled `None`. It is a *different* site from the two item 3 still lists (a slot chunk is ~768
bytes, not the touch kit's 7,680 or the RP2040's 10 KB), so item 3 stays open.
Cost: +288 B flash on `testbench_rp2040`, +320 B on `testbench_rp2350`, no RAM.

**A lane added to `pre-commit` could be silently skipped.** `PARALLEL_LANES` was a hand-written
list (`jvm host arm6 arm8`); a lane added to `LANE_ORDER` but not to it was counted as selected,
printed by `--list`, and never run — which is what the QA rows did on the run that added them.
It is derived from `LANE_ORDER` now. `sim-run.sh` also gained `--no-pull`, which the sim lanes
pass: it was doing an unconditional `git pull --ff-only` on every invocation, which a
verification run must not do, and ten of them would also race on `.git/index.lock`.

**One fix still has no automated check.** `SharedPreferences.commit()` rewriting in place at
the storage cap (`f3712676`) needs an app sitting at its per-app quota, and the quota is only
enforced when a package is running, so the scenario is neither a host test nor a reliable sim
one — a test that tries to fill the volume to a block boundary would be flaky across boards
rather than useful. `qa_store` clears the round's stores on entry, which exercises the path but
asserts nothing about it. The honest next step is a `qa_store` section on a multi-app board
that fills the quota deliberately and then clears, run under the HIL row that already has one.

## 11. Nightly triage, 2026-09-15 (the first night after the round landed)

The 3 AM sim run failed one row and the 4 AM fleet run twelve, across three boards. What each
one was, and what it became:

- **Sim `threadstress`, both shrink modes — a latent JVM race, fixed.** The bisect landed on
  `09d0b1bd` (the simulator's pdb task, 2026-09-14): a real `PRIORITY_RT_1` task polling its
  socket at 100 Hz makes FreeRTOS rotate the equal-priority Java tasks every time it blocks
  (`vTaskSwitchContext` picks the *next* ready entry, not the interrupted one), and the heap's
  "switch only at blocking points" contract had only ever been a scheduling assumption. Slowing
  the poll to 1 Hz merely delayed the failure. Now enforced by a kernel mutex —
  `docs/designs/jvm-run-lock-2026-09.md`. The same rotation exists on a device behind the tick
  timer, the sensor sampler and the USB bridge; `threadstress` passed there by luck.
- **`testbench_rp2040`: `prefs_demo`, `filesdemo` — a full volume, fixed.** Both passed on
  2026-09-13. Root-level writes still worked; every `mkdir` failed. The RP2040's 128 KB LittleFS
  had filled with the `/data/<pkg>` directories of every app flashed before (a metadata pair,
  8 KB, each): `sweep_orphans` was `has_multi_app`-only. It runs on every board now; the first
  boot swept five directories and `filesdemo` passed 29/29 on the bench.
- **`pico_touch_kit`: `helloworld:pdb-settings-uninstall`, both modes — a harness miss,
  fixed.** First run of the pdb rows on that slot. Driven by hand over `pdb input tap`, the
  formula's Uninstall tap (`card_y + 78`) missed and `card_y + 90` uninstalled;
  `lib.sh::settings_dialog_ok` aims at the button's middle now.
- **`pico_touch_kit`: `blinky:pdb-launch[shrink]` — a harness race, fixed.** The launch itself
  worked; the boot-time `pdb list` right after two uninstall reboots timed out (`LIST recv
  failed`), and it is the listing that has to name the launcher. `hil-run.sh` retries that
  listing for up to ~10 s before it counts.
- **`pico_touch_kit`: `quotademo`, both modes — a test assumption, fixed.** The app hard-wired
  the 128 KB cap of the rp2350 boards; the touch kit's is 1 MB. It learns the cap from the first
  `StatFs` (available plus what it already holds) and writes as many blobs as fit.
- **`pico_enviro_mon_w`: `qa_thr`, both modes — the board has no room; row restricted.** Every
  `Thread.start` reported `task spawn failed`; `pdb sysmon` after the run shows min free heap
  16.5 KB, less than one 16 KB Java thread stack, with the Wi-Fi stack and cyw43 buffers
  resident. The row already excluded the touch kit for the same reason; it excludes the W board
  now and the sim covers it. (A lighter `qa_thr`, or Java thread stacks sized per board, would
  put it back — item 3's territory.)
- **`pico_touch_kit`: `qa_ui[no-shrink]` — an empty RTT capture, not reproduced.** Zero log
  lines in 95 s while the shrink run passed minutes later; the probe-recovery power cycle that
  followed is the known cure. Watch for a repeat.
- **`testbench_rp2040`: `helloworld:pdb-install[shrink]` — a reboot that missed the 20 s PING
  window, not reproduced.** `install-stress[shrink]` (ten installs) passed right after it, and
  the no-shrink row passed. Intermittent since 2026-09-12; unchanged.
- **`testbench_rp2040`: `imagedemo`, both modes — an unaligned pixel read, FIXED 2026-09-15.**
  The faulting PC was missing because probe-rs 0.31 catches the hardfault at the *vector*, so
  the handler body never ran; with `--no-catch-hardfault` and a handler that logs the stacked
  frame, it is `transform_rgb565a8` reading `0x101018ab` — an odd address. Papk sections were
  packed back to back, so an ASSETS section could start at 3 mod 4 and carry its (section-
  relative 4-byte-aligned) pixels to an odd flash address; a Cortex-M0+ HardFaults on the
  `uint16_t` read, an RP2350 and the sim do not. The writer aligns every section now and the
  asset registry refuses an unaligned descriptor. Row PASSes 2/2.
  Full write-up: `bugs-rp2040-imagedemo-2026-09-15.md`.
- **`testbench_rp2350` — no run since 2026-09-10.** Not a failure: that slot became the touch
  kit (`fleet.conf`), and the Enviro row accepts `testbench_rp2350` firmware.

Also fixed on the way: `View touched off the UI thread` fired for any native a worker called
(`Thread.currentThread()`, `SystemClock`), because the graphics dispatcher warned before it
knew whether the class was its own. It warns only for a View/Display native now.
