# Bug backlog: on-device memory stress run — 2026-07-23

Findings from a PDB-driven input-injection stress run of `keydemo` and `picoenvmon`
on a real **pico_enviro_mon** board (RP2350, Enviro+ Pack, buttons A/B/X/Y only).
Each section is self-contained: symptom, evidence, repro, suspected root cause,
fix direction, and how to verify the fix. Work them top-down; PEM-1 and PEM-2
touch the same file and are best fixed together.

**Run environment**

- Commit `037b761` (clean tree), firmware `--shrink -r` + `PICODROID_EXTRA_FEATURES=mem-diag`
- Heap: 416 KB FreeRTOS heap_4 (`rp2350.toml heap_kb=416`); separate 48 KB LVGL pool (invisible to memmon)
- Input: `pdb input keyevent` over USB-CDC (non-invasive HAL injection, commit `037b761`)
- Telemetry: 1 Hz `memmon:` RTT lines + `pdb sysmon` JVM block (mem-diag, commits `34cc14f`/`a1ab6ac`)
- Volume: keydemo 1,021 events; picoenvmon 786 events over scenarios (hub cycling,
  3×2 min Live dwells, History enter/exit ×40 + sub-connect racing ×25, AlertDialog
  hammer ×30 + leave-with-dialog ×8, Settings churn ×12, BACK spam ×80, rapid
  cross-screen ×10, 240 s settle), 1,559 soak windows + ~28 min of targeted probes.
- No hangs, crashes, or GC-PRESSURE events anywhere in the run.

**Board key map** (`platforms/rp/boards/pico_enviro_mon/board.toml`):
A=GP12=19/DPAD_UP, B=GP13=20/DPAD_DOWN, X=GP14=23/DPAD_CENTER (LVGL ENTER),
Y=GP15=4/BACK (LVGL ESC). Only these four keycodes resolve. The picoenvmon hub
ListView **wraps** on UP-from-top.

**Reproducing the setup**

```bash
# flash with telemetry (blocks forever; run in background, RTT streams to the log)
setsid env PICODROID_EXTRA_FEATURES=mem-diag \
  ./scripts/flash.sh -b pico_enviro_mon -a picoenvmon --shrink -r > rtt.log 2>&1 &
# wait for "memdiag: ACTIVE" in rtt.log, then drive input:
cargo build --release --manifest-path tools/pdb/Cargo.toml --target x86_64-unknown-linux-gnu
PDB=target/x86_64-unknown-linux-gnu/release/pdb   # ~56 ms/event; pdb.sh works but is ~2 s/event
"$PDB" -s /dev/ttyACM1 input keyevent 23
```

Headless sim equivalent (control channel on stdin — feed ALL commands from one
persistent pipe; a closed writer EOFs the channel permanently):

```bash
( sleep 10; echo "input keyevent 20"; sleep 1; ... ) | \
  PICODROID_SIM_HEADLESS=1 ./scripts/sim.sh --app picoenvmon -b pico_enviro_mon
```

---

## PEM-1 — Keypad ENTER never fires `Button.setOnClickListener` (Settings unsavable on hardware) · **Critical, functional**

### Symptom

Pressing X (DPAD_CENTER → LVGL ENTER) on a focused `picodroid.widget.Button`
never invokes its `OnClickListener`. In picoenvmon this makes the Settings Save
button dead: thresholds/units can never be persisted from the device's only
input method. Any app using `Button` + keypad is equally affected.

### Evidence

- `Settings saved:` (logged by `SettingsActivity.commit()`,
  `examples/picoenvmon/java/picoenvmon/ui/settings/SettingsActivity.java:135`)
  appeared **0 times** in the entire run: 6 in-soak Save presses + two on-device
  sweeps pressing X at **every** focus position (0–5 DOWNs from screen entry,
  twice) + a headless-sim run = **0 / 44** attempts.
- The headless sim failed **on the very first Settings open after boot** — no
  prior Activity had been destroyed, so this is not state decay or map
  exhaustion; the path never works.
- Working X-activation on the same board/input path (rules out injection, key
  routing, and the focus-group machinery):
  - ListView row activation: hundreds of hub opens (`Home` → `*.onCreate` markers).
  - AlertDialog opening from History rows: during the 30× dialog hammer the
    allocation rate rose ~+5 allocs/window over hub baseline — the dialogs
    (Builder + 2 string concats each) really opened.
  - Switch toggle: X on the Live Logger switch fired `Logger started`
    (`LiveActivity.java:156`) — this is the compound-button
    (`LV_EVENT_VALUE_CHANGED`) path, not the click path.

### Repro (device or headless sim)

From a fresh boot of picoenvmon: `keyevent 20 ×2` (hub → Settings row),
`keyevent 23` (open), `keyevent 20 ×4` (temp→hum→lux→Units→Save),
`keyevent 23`. Expect `Settings saved: …` in the log; observe nothing.
Sweeping X across every other focus position also produces nothing.

### Suspected root cause

The generic View-click pipeline in
`platforms/rp/src/system/picodroid/graphics/lvgl/widgets/button.rs`:
`register_click_listener()` (attaches an `LV_EVENT_CLICKED` trampoline +
`CLICKABLE` flag on first registration) → `view_click_cb` → `CLICK_QUEUE` →
lifecycle drain → `performClick`.

Verified-correct pieces (don't re-investigate):

- Group wiring: `push_activity_group()` runs **before** `onCreate`
  (`platforms/rp/src/lifecycle.rs:428`) and binds the keypad indev
  (`events.rs:226-268`); `lv_button` and `lv_switch` are `group_def=TRUE` in the
  vendored LVGL (`vendor/lvgl/src/widgets/button/lv_button.c:35`) so the Save
  button auto-joins the active group in creation order
  (temp, hum, lux pickers → units switch → save button).
- Java→native registration: `View.setOnClickListener` →
  `nativeRegisterClickListener` → `view.rs:268 register_click_listener` — intact,
  no `native miss` logged.

Prime suspect: the keypad-ENTER press/release synthesis never produces
`LV_EVENT_CLICKED` on the focused `lv_button` (the working widgets all listen to
*other* events — item-click, VALUE_CHANGED, dialog-internal). Check what the
keypad `read_cb` (`events.rs` `keypad_read_cb`, GPIO-edge mirror) reports for
ENTER press/release state transitions and whether LVGL's keypad indev turns that
into PRESSED→RELEASED→CLICKED on the focused object, vs only `LV_EVENT_KEY`.
Touch boards presumably exercise clicks via pointer indev, which is why this was
never caught — this appears to be the first Button+keypad exercise on a
buttons-only board.

### Fix verification

Device: repro sequence above → expect `Settings saved: tempHi=… ok=true`, a
"Saved" Toast, and `finish()` back to the hub. Sim: same headless script asserts
one `Settings saved:` per Save press, including on 2nd+ Settings opens (see
PEM-2 — re-opens exercise the address-reuse path). Also X-activate keydemo's
"Focus me" button after giving it an OnClickListener in a scratch test if a
framework-level unit is wanted.

---

## PEM-2 — Native listener maps are append-only; dead view graphs stay GC-pinned; silent callback loss on address reuse and map overflow · **Medium (memory) with Critical latent cliffs**

### Symptom (measured)

Matched-state hub floor (post-GC JVM live floor sampled at identical app state
between scenarios) ratcheted **11,487 → 21,568 B** in exact, scenario-correlated
steps, then plateaued hard:

```
hub-S1    w=264   floor=11487   (after Live visits 1-2)
hub-S2-1  w=401   floor=11487
hub-S2-2  w=538   floor=14423   (+2936 — Live visit 3)
hub-S2-3  w=674   floor=17359   (+2936 — Live visit 4)
hub-S3a-10 w=729  floor=19868   (+2509 — early History cycles)
hub-S3a-20..S4b   floor=19988   (flat across 55 more History cycles + 38 dialogs)
hub-S5-10 w=1149  floor=21715   (+1727 — Settings cycles)
hub-S6..S8-final  floor=21568   (flat; still exactly 21568 at w=3239, ~90 min
                                 and 10 more Live opens + 14 Settings opens later)
```

### Root cause (code-confirmed)

`platforms/rp/src/system/picodroid/graphics/lvgl/widgets/button.rs`:

- `VIEW_CLICK_MAP` / `VIEW_LONG_CLICK_MAP`: `[(usize raw_lv_obj_ptr, u16 obj_ref); 32]`,
  append-only (`VIEW_CLICK_MAP_LEN += 1`, lines ~29-43, 118-146). The only reset
  is `reset_button_state()` (line ~251) at app restart. Same pattern for the
  checked-change map in `widgets/switch.rs` (and its
  `visit_checked_change_listener_roots`).
- Entries are **GC roots by design** (the d3e052d fix that keeps listener-only
  views alive), so a destroyed Activity's registered widget + its listener
  lambda + everything the lambda captures (the Activity instance, its whole view
  tree — e.g. Save's `v -> commit()` captures `SettingsActivity`) stays live
  after `finish()`.
- The ratchet *plateaus* only by allocator coincidence: LVGL reuses freed
  `lv_obj` addresses, and re-registration with a matching `raw_ptr` takes the
  **update path** (`entry.1 = obj_ref; return;`) which overwrites the stale
  `obj_ref`, finally releasing the old graph. ~2.9 KB per pinned Live graph
  measured; worst case is bounded by map capacity, not by design.

Two latent cliffs in the same code:

1. **Update path never re-attaches the LVGL callback.** The trampoline was
   registered on the *old* widget instance and died with it; a new widget at a
   recycled address gets a map entry but no `lv_obj_add_event_cb` → silently
   unclickable. Once PEM-1 is fixed this becomes the next way Buttons die
   (2nd+ visit to any screen, on touch boards too).
2. **Silent overflow at 32 entries**: `if VIEW_CLICK_MAP_LEN < MAX_CLICK_VIEWS`
   simply drops the registration — no log, no error. Clicks dead with zero
   diagnostics.

### Fix direction

Unregister on widget/view destroy (the view-delete path that already handles
`cancel_subtree` for animations is the natural hook), or key entries by
something stable + validate liveness at dispatch; always (re)attach the event
callback on registration regardless of map-hit; log (defmt) on overflow.
Mirror the same treatment in `switch.rs` (checked-change) and the key/touch
maps if they share the pattern.

### Fix verification

Under `mem-diag` firmware, loop Live enter→dwell 20 s→exit ×6 and Settings
enter/exit ×6 from the hub, with 20 s hub checkpoints between: hub floor must
return to its post-first-visit value every cycle (no +2.9 KB steps at visits
3-4). Then hammer one screen enter/exit ×40: clicks (once PEM-1 works) must
still fire on the 40th visit (exercises address reuse), and floor stays flat.

---

## PEM-3 — Heap fragmentation degrades 2.6× under navigation stress and never recovers · **Medium**

### Symptom (measured, memmon fields)

Over one ~40 min nav soak at steady total-free levels:

- Largest free block `lblk`: **297 KB → 133 KB** (−55%)
- Fragmentation `frag` (permille of free space NOT in the largest block):
  **3‰ → 351‰**
- Min-ever native free `nmin`: 296 KB → 170 KB

Step degradations correlate with scenario boundaries; the single worst step was
during rapid cross-screen transitions (S7: `lblk` 174,744 → 133,200 within ~30
windows). No recovery during a 240 s settle nor the following ~90 min — heap_4
coalescing cannot help because long-lived allocations made mid-churn
(ChunkedSlots chunks, fields-arena growth steps, PEM-2 pinned graphs) are
stranded mid-heap.

### Why it matters

This is the most plausible eventual-OOM mechanism for the 416 KB target: a
large allocation (LVGL render buffer, arena growth step, PAPK install scratch)
fails despite ample *total* free. picoenvmon is already ~30 KB short of budget
(see `project_picoenvmon_heap_budget` memory / board.toml notes).

### Repro / measurement

Any create/destroy-heavy nav loop under mem-diag firmware; watch `lblk`/`frag`
in the memmon line at matched idle states. The S7 pattern (Live→back,
History→back, Settings→back ×10, ~300 ms gaps) is the strongest single driver.

### Fix direction (investigation order)

1. Fix PEM-2 first — pinned graphs stranded mid-heap are direct fragmenters.
2. Then re-measure. If still degrading: look at allocation lifetime mixing —
   e.g. ChunkedSlots chunk growth during screen churn interleaves permanent
   chunks with transient Activity allocations. Options: chunk pre-reservation
   at app start, size-class pooling for recurring transient sizes, or freeing
   empty chunks on GC (they currently persist).

### Fix verification

Same soak; `lblk` at the final hub settle should stay ≥ ~250 KB and `frag`
under ~100‰, with no monotonic step pattern across scenario boundaries.

---

## PEM-4 — `SensorLoggerService` ALERT path: unconditional per-second log/alloc churn, no edge detection · **Low-Medium**

### Symptom (measured)

**11,319** `ALERT:` lines in ~63 min of logger-on time — exactly 3/s (3,773 each
for temperature, humidity, light). With default thresholds all three breach
continuously indoors (light 0.0 lx, humidity below `humLo`, temp above
`tempHi`), and the service re-logs every breach on every 1 Hz smoothed emit:

```
[INFO ] SensorLogger: ALERT: humidity below threshold: 39.243999 m%
[INFO ] SensorLogger: ALERT: light below threshold: 0.0 lx
```

Cost: ~13 JVM allocs/s **at idle** (hub baseline 15 allocs/window with the
logger on vs 0 with it off — string concat + boxing per ALERT), a GC every
~16 s forever (146 GCs in the picoenvmon soak vs 15 in keydemo's), and an RTT
log flooded to the point of burying real signals.

### Code

`examples/picoenvmon/java/picoenvmon/service/SensorLoggerService.java:155-165`
(`onSensorChanged` breach branches).

### Fix direction

Alert on breach **transition** (edge detection per sensor: log once on
entering-breach, optionally once on clearing), or rate-limit (e.g. ≥60 s
between repeats per sensor). Note the RGB LED IAQ drive (`applyLedFromIaq`) is
separate and fine.

### Fix verification

mem-diag soak at the hub with logger on: hub-idle windows should read
`alloc=+0 nalloc=+0` (or near), matching the logger-off baseline; ALERT lines
appear only on threshold crossings.

---

## PEM-5 — mem-diag native growth sentinel never re-arms after the first Activity → LEAK? warnings are noise on multi-screen apps · **Low (diagnostics)**

### Symptom (measured)

**194** `memmon: LEAK? native floor rose …` warnings during the picoenvmon
run — every one comparing against `baseline 127160 B`, the arm point after the
**first** Activity (Home) settled:

```
[WARN ] memmon: LEAK? native floor rose +42104 B over 8 windows (baseline 127160 B, now 182848 B)
[WARN ] memmon: LEAK? native floor rose +9936 B over 8 windows (baseline 127160 B, now 192488 B)
```

Every later screen's legitimate construction plus the churn-driven native
sawtooth (garbage accumulating between allocation-paced GCs during Live dwells
produces ≥7/8 rising windows) re-trips it indefinitely. Meanwhile the **JVM**
floor sentinel produced zero warnings all run — correct, since the only real
retention (PEM-2) stepped below its 4,096 B threshold.

The net effect: on any multi-Activity app, device `LEAK?` (native) lines carry
no signal, which will train people to ignore the one warning the monitor can
emit on hardware.

### Code

Sentinel arming/baseline logic in `platforms/rp/src/system/mem_diag.rs` (arming
described in `docs/memory-diagnostics.md` §"The growth sentinel": arms after
onCreate + 2 settle windows — but only once, for the first Activity).

### Fix direction

Re-arm the native baseline after each Activity transition (the lifecycle
already tells mem_diag when `on_tick` context changes), or track the baseline
against the post-GC native floor rather than raw `nused`, or suppress re-trips
while the JVM-side allocation rate shows active churn. Keep the JVM-floor
sentinel exactly as is.

### Fix verification

Re-run the picoenvmon soak: expect zero (or single-digit, transition-adjacent)
native LEAK? warnings on a build where PEM-2/PEM-4 are fixed, while a
deliberately-injected native leak (selftest-style ramp) still trips it.

---

## Minor observations (not scheduled bugs)

- **OBS-1 — No idle GC:** GC is allocation-paced only; after input stops, dead
  garbage (≈4 KB of strings in keydemo's case) sits unreclaimed indefinitely.
  Only matters for heap-capped apps that park idle right after a churn burst.
  Possible cheap fix: one GC after N consecutive zero-alloc windows.
- **OBS-2 — `pdb sysmon` output interleaving:** the host CLI prints the task
  table header, then the JVM mem-diag block, then the task rows underneath it.
  Cosmetic ordering bug in `tools/pdb/src/sysmon.rs`.

---

## What was validated (no action needed — regression baselines)

- **keydemo / input pipeline clean:** exactly 4 allocs per keypress (the two
  Java `String` concats in `KeyDemoActivity.onKey` × DOWN+UP); `nalloc` total 1
  across 1,021 events → the recycled-KeyEvent zero-alloc dispatch (e988eb1)
  holds end-to-end through PDB→HAL→LVGL→Java. Floor flat at 800 B; GC ≤1/window;
  frag settled 4‰. BACK ×50 correctly consumed by `onKey` without finishing.
- **BACK spam at hub is allocation-free** (80×: 15.5 allocs/window = the PEM-4
  service churn exactly; the key path itself adds nothing).
- **No recurrence** of previously fixed bugs under targeted probes:
  History stale-`onServiceConnected` UAF (c251f22) — 25 sub-connect racing
  exits clean; AlertDialog leak on Activity leave (bcb22ba) — 8 leave-with-dialog
  cycles clean; listener-View GC sweep (d3e052d) — input alive through 2,400+
  events; no dangling-handle hang.
- **Live sensor pipeline:** 1 Hz × 5 tiles ≈ 29 allocs+interns/window while
  dwelling — GC absorbs it at ~1 cycle/90 s with flat floor (churn is by
  design; not flagged).

## Raw data

Session scratchpad (may not survive cleanup — key excerpts are inlined above):
`/tmp/claude-1000/-home-shiv-projects-picodroid-rs/a8c969c1-988b-4a0a-9c71-5bfa35bf7487/scratchpad/`
— `envmon.rtt.log`, `keydemo.rtt.log` (full RTT), `*.driver.log`
(`EV|t0_ms|t1_ms|memmon_w|label|rc|args` per injected event), `*.sysmon.log`,
`analysis/*.memmon.tsv` (parsed telemetry), `sim-save-test.log` (PEM-1 sim
repro), `lib-soak.sh` / `drive-keydemo.sh` / `drive-envmon.sh` /
`parse-memmon.sh` (reusable harness).
