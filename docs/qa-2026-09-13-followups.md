# Open Follow-ups — QA round 2026-09-13

Everything the 2026-09-13 QA round left open (`qa-2026-09-13.md`: seven `qa_*` apps, forty
fixes, three boards). Ordered by risk. Each entry says what was seen, what is already ruled
out, and the next concrete step, so it can be picked up cold.

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

## 2. Devices run the legacy handle cast — soak `handle-table-32` (P1)

Every bench board casts `lv_obj_t*` to a 32-bit handle, so the sim's stale-handle answers
(`Reparent::StaleChild`, `is_live`, the sanitizer) do not exist there: a stale widget handle
is a use-after-free. F10 moved the released state into Java (`View.release()`, refused by
`addView`, `IllegalStateException` on a released receiver) so the app-driven cases are
covered, but a second `AlertDialog.dismiss()` after a keypad BACK is still a dangling pointer
on a device. The generation-tagged table is staged behind the default-off `handle-table-32`
feature (`designs/handle-table-invalidation.md`).

**Next step.** Build each board with `PICODROID_EXTRA_FEATURES=handle-table-32` (honoured by
`lib.sh`'s firmware build, so by `flash.sh`; `hil-run.sh`'s own cargo line does not read it yet
and needs the same one-line change), run the `qa_ui` row (its `tree` and `dialogs` sections are the ready-made check) and the picoenvmon
soak, measure the RAM cost, then default it on. The QA round's hardware runs are the baseline.

## 3. Unchecked allocations left in native paths (P2 — a board reset each)

J23–J26 made the formatter, file streams, frames and interning fallible; three infallible
sites remain, each a reset instead of an `OutOfMemoryError`:

- **Touch kit, 7680 bytes** on the path a background-pool worker takes into Java under a full
  heap (`qa_thr` on `pico_touch_kit` without the conf restriction). It is not the frame or the
  task stack. A device heap census (`docs/memory-diagnostics.md`, `--mem-diag` on the sim
  first) is the way to name it.
- **RP2040, 10 KB** in `qa_ui`'s focus section once the containers section has exhausted the
  heap (`qa_ui` is restricted to the RP2350 boards in `hil-tests.conf` for this reason).
- **LittleFS file cache, 4 KB per open** (`fs/hal_impl.rs`): on the RP2040, once `qa_store`
  had filled the volume and thrown a dozen `IOException`s, the next open reset the board with
  "memory allocation of 4096 bytes failed". The stream arms could reserve it fallibly like
  the read buffer does since J24.

The rule the round established: the sim's device heap model is the RP2350's; the RP2040
(160 KB) and the touch kit (LVGL draw buffers) are tighter, so every infallible allocation in a
native path is a reset waiting for a big enough app. A source scan for `to_vec`/`Vec::with_capacity`/
`format!` in native arms (in the spirit of `crates/test_support/source_scan.rs`) would keep the list
from growing.

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

## 5. A foreign LittleFS geometry should format, not fail to mount (P3)

When the Pico 2 W slot booted a touch-kit image (see 6), its LittleFS region was left
formatted for that board's geometry; the testbench firmware then boots with
`[fs] init failed: invalid parameter` and every open fails — `qa_life`'s prefs and a whole
`qa_store` run — until `probe-rs erase --chip RP235x`. The runtime could treat a geometry
mismatch like a missing superblock and format the volume (with a log line), as it does for a
blank chip. Cheap; the `bootcount` example plus a power cycle verifies it.

## 6. Harness: same-family boards must not run `hil-run.sh` in parallel from one worktree (P3)

Two `hil-run.sh` invocations for boards of one MCU family share
`target/<triple>/release/picodroid`, so one board can be flashed with the other's image (the
Pico 2 W once booted a touch-kit build, probing PSRAM and a GT911 that are not there).
`hil-fleet.sh` avoids it with a target directory per slot; a bare `hil-run.sh` does not.
Give `hil-run.sh` the same per-slot `CARGO_TARGET_DIR` (or make it refuse to start while a
same-family run holds the ELF), so the nightly's safety extends to one-off runs. The RP2040
(thumbv6m) is safe alongside either RP2350 board.

## 7. `IllegalFormat*` exception names (P3)

The formatter now distinguishes the cases (J19c) but throws the family's base class,
`IllegalFormatException`, for a precision on an integral conversion. Java throws
`IllegalFormatPrecisionException`; giving the family its class names is a registry entry
each (`project_class_name_to_static`, `PICODROID_NATIVE_CLASSES`), and the `qa_coll`
divergence log line is the test.

## 8. Expectations to document, not fix

- The RP2040's 160 KB heap holds far fewer boxed entries than the sim's device model; an app
  that grows a collection past a few hundred boxes hits `OutOfMemoryError` there first. The
  QA apps are sized for it; the sizing rule belongs in the website's storage/limits page.
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
