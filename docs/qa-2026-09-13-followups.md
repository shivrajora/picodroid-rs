# Open Follow-ups — QA round 2026-09-13

Everything the 2026-09-13 QA round left open (`qa-2026-09-13.md`: seven `qa_*` apps, forty
fixes, three boards). Ordered by risk. Each entry says what was seen, what is already ruled
out, and the next concrete step, so it can be picked up cold. Status as of 2026-09-14
(branch `qa-followups-2026-09-14`): items 3 (in part), 4, 5, 6, 7 and 8 are landed, each as
one commit; item 2 is soaked on the bench and awaits the picoenvmon soak; item 1 stays open.

## 1. `qa_life` stalls on the Pico 2 W slot (P1 — a lost main-executor post)

**Seen.** `hil-run.sh --app qa_life --board testbench_rp2350` (the Pico 2 W in the
`pico_enviro_mon_w` slot, testbench firmware) stalls about one run in two, in both shrink
modes, after a Service re-creation: the app's next step is a `Runnable` posted to
`Executors.mainExecutor()` from a child thread. With the timer instrumented the post is armed
and queued (no "queue full" drop, no Runnable error now that F11 logs them) and the UI loop
never runs it. The runs that stall are the ones whose pending-op drains took 100–260 ms just
before.

**Ruled out.** RTT back-pressure (a `rtt-lossy` build, non-blocking `defmt-rtt`, stalls the
same way); the flash volume (a freshly erased and formatted volume stalls the same way); the
app (the sim, `testbench_rp2040` and `pico_touch_kit` never stall, 74/74 every run).

**Suspect.** The main task does not wake from `recv_blocking` for a child task's post on that
board only. What sets the board apart is its second core carrying the cyw43 task and the flash
parker — the same neighbourhood as the reverted WP7 tick-clock change and its W-board tick
stall (`scheduling-audit-2026-09.md`).

**Next step.** Reproduce with `pdb` attached (`scripts/pdb.sh --board testbench_rp2350`) and
take a stack dump of the main task mid-stall: is it blocked in `recv_blocking` with a
non-empty queue (a lost wake), or somewhere else (a held lock, the parker)? Then instrument
the queue's send side with the sender's core. A sim reproduction is unlikely (single core).

**2026-09-14.** One point the code settles before any dump: a lost wake alone cannot stall
the loop. `lifecycle.rs` blocks in `recv_blocking` with no timeout, but the tick source posts
an `LvglTick` every 16 ms and that wake drains the same queue, so a queued Runnable would run
on the next tick. A stall therefore means the main task is not being scheduled at all (no
ticks either), or the queue send from the child never landed. `pdb sysmon` shows every task's
state and stack head-room and is served off the main task, so it answers the first question
without a stack dump: a main task in `Ready` while nothing runs it points at a lost
cross-core yield after the flash parker's XIP-off window (prefs writes precede the step that
stalls); `Blocked` points at the queue. The reproduction loop used (flash `qa_life -r`,
watch RTT, `sysmon` when the log goes quiet for 20 s) is in the session notes below.

## 2. Devices run the legacy handle cast — soak `handle-table-32` (P1)

Every bench board casts `lv_obj_t*` to a 32-bit handle, so the sim's stale-handle answers
(`Reparent::StaleChild`, `is_live`, the sanitizer) do not exist there: a stale widget handle
is a use-after-free. F10 moved the released state into Java (`View.release()`, refused by
`addView`, `IllegalStateException` on a released receiver) so the app-driven cases are
covered, but a second `AlertDialog.dismiss()` after a keypad BACK is still a dangling pointer
on a device. The generation-tagged table is staged behind the default-off `handle-table-32`
feature (`designs/handle-table-invalidation.md`).

**Next step.** Build each board with `PICODROID_EXTRA_FEATURES=handle-table-32` (honoured by
`lib.sh`'s firmware build and, since `e74655b4`, by `hil-run.sh`), run the `qa_ui` row (its
`tree` and `dialogs` sections are the ready-made check) and the picoenvmon soak, measure the
RAM cost, then default it on. The QA round's hardware runs are the baseline.

**2026-09-14.** `qa_ui` passes with the table on both RP2350 boards in both shrink modes
(`pico_touch_kit` 146/146 ×2, `testbench_rp2350` on the Pico 2 W slot 141/141 ×2). Cost
against the size baseline: +2,400 B flash / +1,032 B RAM on `testbench_rp2040`, +3,352 B
flash / +1,024 B RAM on `testbench_rp2350` (the table itself is the kilobyte of `.bss`).
Left: the picoenvmon soak (`PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire
--board pico_enviro_mon_w --pin`, then `hil-run.sh --app picoenvmon` under the feature, or the
overnight recipe in `project_pdb_hw_soak_harness`); then flip the default in
`platforms/rp/Cargo.toml` and accept the size baseline in the same commit.

## 3. Unchecked allocations left in native paths (P2 — a board reset each)

J23–J26 made the formatter, file streams, frames and interning fallible; three infallible
sites remain, each a reset instead of an `OutOfMemoryError`:

- **Touch kit, 7680 bytes** on the path a background-pool worker takes into Java under a full
  heap (`qa_thr` on `pico_touch_kit` without the conf restriction). It is not the frame or the
  task stack. A device heap census (`docs/memory-diagnostics.md`, `--mem-diag` on the sim
  first) is the way to name it.
- **RP2040, 10 KB** in `qa_ui`'s focus section once the containers section has exhausted the
  heap (`qa_ui` is restricted to the RP2350 boards in `hil-tests.conf` for this reason).
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
improvement. The two device findings above (7680 bytes on the touch kit, 10 KB on the
RP2040) are still open and still need the heap census to name.

## 4. RP2040 negative-fraction casts floor framework-wide (P2)

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

## 5. A foreign LittleFS geometry should format, not fail to mount (P3)

When the Pico 2 W slot booted a touch-kit image (see 6), its LittleFS region was left
formatted for that board's geometry; the testbench firmware then boots with
`[fs] init failed: invalid parameter` and every open fails — `qa_life`'s prefs and a whole
`qa_store` run — until `probe-rs erase --chip RP235x`. The runtime could treat a geometry
mismatch like a missing superblock and format the volume (with a log line), as it does for a
blank chip. Cheap; the `bootcount` example plus a power cycle verifies it.

**Done** (`5dffb2ab`): `fs/volume.rs` formats on `Invalid` as on `Corrupt`, with a
`[fs] mount failed: foreign superblock (geometry or version); formatting the volume` warn;
host tests cover a clean remount (keeps files) and a foreign block count (formats).

## 6. Harness: same-family boards must not run `hil-run.sh` in parallel from one worktree (P3)

Two `hil-run.sh` invocations for boards of one MCU family share
`target/<triple>/release/picodroid`, so one board can be flashed with the other's image (the
Pico 2 W once booted a touch-kit build, probing PSRAM and a GT911 that are not there).
`hil-fleet.sh` avoids it with a target directory per slot; a bare `hil-run.sh` does not.
Give `hil-run.sh` the same per-slot `CARGO_TARGET_DIR` (or make it refuse to start while a
same-family run holds the ELF), so the nightly's safety extends to one-off runs. The RP2040
(thumbv6m) is safe alongside either RP2350 board.

**Done** (`fb0d1094`): with a fleet config a bare run defaults to the nightly's
`build/hil/<slot>/{target,apks}`; the item-2 runs above went on both RP2350 slots at once.

## 7. `IllegalFormat*` exception names (P3)

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
nightly.

