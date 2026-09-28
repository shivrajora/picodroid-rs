# Open Follow-ups — QA round 2026-09-13

Everything the 2026-09-13 QA round left open (`qa-2026-09-13.md`: seven `qa_*` apps, forty
fixes, three boards). Ordered by risk. Each entry says what was seen, what is already ruled
out, and the next concrete step, so it can be picked up cold. Status as of 2026-09-14
(branch `qa-followups-2026-09-14`): items 3 (in part), 4, 5, 6, 7 and 8 are landed, each as
one commit; item 2 is soaked on the bench and awaits the picoenvmon soak. **Item 1 — the last
open P1 — was found and fixed on 2026-09-15**; it was an SPI transfer that never signalled
completion and was waited out silently for five seconds at a time, not a lost executor post.
Section 9 records the regression coverage every fix of the round now has.

**Status 2026-09-16:** items 1, 2, 3, 4, 5, 6 and 7 are closed and item 8 is documentation. Item
3's last site, the touch kit's 7,680 B, was named and fixed on 2026-09-16 (the lambda registry
doubling under a burst of `execute(() -> …)` posts). Smaller residues, none of them
blocking: which cause drops the SPI byte in item 1 (its "Left open" paragraph, now in
[completed/qa-2026-09-13-followups.md](completed/qa-2026-09-13-followups.md)), and why the RP2040 does not show it; an
automated check for `SharedPreferences.commit()` at the quota (§10); `qa_thr` restricted off the
W board for want of heap (§11); and two unreproduced one-offs to watch in the nightly — the touch
kit's empty RTT capture and the RP2040's `pdb-install[shrink]` reboot missing its PING window (§11).

Completed items: [completed/qa-2026-09-13-followups.md](completed/qa-2026-09-13-followups.md) — items 1 (`qa_life` SPI stall), 2 (handle-table flip), 3 (unchecked native allocations), 4 (RP2040 float casts), 5 (foreign LittleFS geometry), 6 (`hil-run.sh` per-slot target dir), 7 (`IllegalFormat*` names).

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
nightly. (It did, on 2026-09-15, and item 1 is now closed — see it in
[completed/qa-2026-09-13-followups.md](completed/qa-2026-09-13-followups.md).)

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
bytes, not the touch kit's 7,680 or the RP2040's 10 KB), so item 3 stayed open at the time
(both closed since, see §3 in
[completed/qa-2026-09-13-followups.md](completed/qa-2026-09-13-followups.md)).
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
