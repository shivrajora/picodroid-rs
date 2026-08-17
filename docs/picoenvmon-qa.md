# picoenvmon — Simulator QA Report

- **App:** `picoenvmon` (Pimoroni Enviro+ environmental monitor)
- **Board:** `pico_enviro_mon` (ST7789 display, no touch; 4 buttons A/B/X/Y on GP12–15)
- **Date:** 2026-06-04 · **Commit:** `51c341f` (v0.10.0)
- **Method:** host simulator via `./scripts/sim-remote.sh --board pico_enviro_mon --app picoenvmon`,
  driven over the control FIFO (`tap A/B/X/Y`) with frames captured by `scrot` on the Xvfb display.

The 4-button nav model is: **A = up, B = down, X = open/activate, Y = back** (legend shown on every
screen). Screens: **Home** (hub menu: Live / History / Settings), **Live** (5 sensor tiles + Logger
switch), **History** (temp sample list), **Settings** (3 threshold fields + units switch + Save).

## Summary

| # | Severity | Area | Issue | Status |
|---|----------|------|-------|--------|
| 1 | 🔴 Critical | JVM GC / navigation | After the first GC, every newly-opened Activity throws `NoSuchMethod` and renders broken | **Fixed** — unrooted `Display` singleton (see below) |
| 2 | 🟠 High | Live / Switch | Logger toggle (X) never fires `OnCheckedChangeListener`; the logging service never starts/stops | **Fixed** — same swept-obj_ref cause as #1 |
| 3 | 🟠 High | History | List never shows data; X→Info dialog unreachable | **Fixed** — via #2 (logger persists) + clearer empty state |
| 4 | 🟡 Low | Fonts | Em-dash `—` and ellipsis `…` render as tofu (`□`) | **Fixed** — ASCII in the 3 rendered strings |
| 5 | 🟡 Low | Settings / EditText | Field clears its displayed value when edited; QWERTY keyboard on a numeric field | **Fixed** — one-line EditText + numeric inputType |
| 6 | 🟡 Low | Settings | Hint bar overflows: "Y:Back" clipped to "Y:B" | **Fixed** — shortened the hint |
| 7 | ⚪ Nit | Home | Menu highlight is teal on first render, blue after any navigation | **Fixed** — also style LV_STATE_FOCUS_KEY |

---

## 1. 🔴 Critical — App breaks after the first GC (`NoSuchMethod` on every new screen)

**Symptom.** After the JVM runs its **first garbage collection**, every subsequently-opened Activity
logs `Activity lifecycle error: NoSuchMethod` during `onCreate` and renders broken: Live tiles stay
as `□` placeholders and never fill, History sticks on "Connecting…", and BACK no longer recovers the
wedged screen. Reproduced **3×**; it breaks by the **2nd–3rd screen transition** — sooner with more
interaction, because that allocates faster and trips the GC sooner.

**Evidence (shutdown log of one session).** `gc: 1 collections, 2327 freed` lines up exactly with the
onset:

```text
[PicoEnvMon] Settings.onCreate          ← nav #1  OK
[PicoEnvMon] History.onCreate           ← nav #2  OK (samples=0)
[PicoEnvMon] Live.onCreate              ← nav #3
[sim] Activity lifecycle error: NoSuchMethod    ← fails here
[sim] JVM ... gc: 1 collections, 2327 freed     ← the one GC that broke it
```

In isolation each screen works fine as the *first* navigation, so this is cumulative, not
screen-specific.

**Root cause (confirmed by runtime tracing).** The JVM GC ([jvm/src/gc/mod.rs](../jvm/src/gc/mod.rs))
is non-moving mark-sweep with slot reuse; class/method tables are append-only and never GC-managed. So
`NoSuchMethod` means a *still-referenced* object was swept (a missing GC root), its `u16` heap slot was
reused by a later allocation, and a subsequent dispatch hit the wrong class's vtable.

The swept object is the **`Display` singleton.** `Display.getInstance()`
([sdk/.../graphics/Display.java](../sdk/java/picodroid/graphics/Display.java)) is a native method that
caches the singleton's heap slot in a Rust `DISPLAY_INSTANCE` cell
([display.rs:20](../platforms/rp/src/system/picodroid/graphics/display.rs#L20)) and hands the same
`ObjectRef` back every call; nothing on the Java side keeps a field to it. That cell was **not visited
by `gc_visit_roots`**, so the first GC swept the Display. Its slot was then reused by a transient
`SensorEvent` (the Live screen's service emits them continuously), and `get_instance`'s staleness
check was only `is_live(existing)` — which *passes* on the reused slot. So `Display.getInstance()`
returned a `SensorEvent`. Every `Activity.setContentView(root)` calls
`Display.getInstance().setContentView(root)`, so the next Activity's `onCreate` resolved
`setContentView` on `SensorEvent` → `NoSuchMethod`. Runtime trace at the failure:
`recv_class=picodroid/hardware/SensorEvent method=setContentView`, frame = `picodroid/app/Activity`.

This is the same class of bug the v0.10.0 fix addressed for the click/key/touch/dialog View maps
(memory `project_gc_collects_unfielded_callback_views`); the `Display` singleton and several widget
listener maps were simply missed.

**Fix.** Root the Display singleton: add
[`display::visit_gc_roots`](../platforms/rp/src/system/picodroid/graphics/display.rs) (visits
`DISPLAY_INSTANCE`) and call it from `gc_visit_roots`; also harden `get_instance` to verify the cached
slot is still a `Display` (re-allocate if a future regression lets it be reused). As defense-in-depth
for the same bug class, also added `visit_*_roots` to the **Switch / CheckBox / ToggleButton /
EditText** listener maps (their local-only widgets had the identical missing-root hazard) and wired
all four into `gc_visit_roots`. Regression test: a jvm-layer test that the GC honors the `extra_roots`
hook (`gc_retains_object_via_extra_roots` / `gc_collects_object_when_extra_roots_omits_it` in
[jvm/src/gc/tests.rs](../jvm/src/gc/tests.rs)). **Verified:** a Live→History→Settings→Live walk across
**5 GC collections** now produces **0** `NoSuchMethod` and every screen renders/binds correctly.

> Note: the unit tests guard the GC mechanism, not the per-widget/Display wiring — the `graphics`
> module is `#[cfg(not(test))]` so it can't be host-unit-tested. The wiring is covered by the sim walk
> above.

---

## 2. 🟠 High — Logger toggle never starts/stops the service (Live) — FIXED

Pressing **X** on the focused Logger `Switch` toggled it *visually* but the `OnCheckedChangeListener`
never fired, so the foreground logging service never started/stopped.

**Same root cause as #1, confirmed by tracing.** The `value_changed_cb` *did* queue and the drain ran,
but the Java `Switch` object's obj_ref was in the **unrooted Switch listener map**, so a GC during
Live's heavy allocation swept it; its slot was reused, and `fireCheckedChanged` then dispatched
`onCheckedChanged` on a wrong-class object — failing silently (`let _ = invoke_instance(...)`). The
widget-map rooting added in the GC-fix commit keeps the `Switch` alive, so the listener now fires.
Verified: 5 consecutive toggles → 5 `Logger started`/`stopped` + `foreground started`/stopped
transitions, including the first deliberate toggle on a freshly-opened Live. (The Settings units
`Switch` shares this path and is likewise fixed.)

## 3. 🟠 High — History never shows data; Info dialog unreachable — FIXED

History always displayed **"No samples yet"** because the `SensorLoggerService` was only ever *bound*
(it died on screen-leave, resetting its ring buffer), so `onServiceConnected`'s one-shot snapshot
always read 0 samples. The root cause was bug #2: the Logger toggle was broken, so the service could
never be promoted to a persistent foreground/started service.

With bug #2 fixed, the intended flow works: turn on **Logger** in Live → the service runs in the
foreground and survives screen changes → opening **History** binds the *same* running service and its
snapshot returns the accumulated ring. **Verified:** `History bound, samples=60`, the list renders the
recent 12 rows, and **X → Info `AlertDialog`** ("Sample N / Temperature: …") now opens and dismisses
with Y. Also softened the empty state to point at the Logger toggle
([HistoryActivity.java](../examples/picoenvmon/java/picoenvmon/ui/history/HistoryActivity.java)).
(A live in-place refresh while History is foreground was considered but rejected: rebuilding the
`ListView` resets the D-pad focus to the top each tick, breaking row navigation.)

## 4. 🟡 Low — Missing-glyph tofu for `—` and `…` — FIXED

The em-dash `—` (U+2014) and ellipsis `…` (U+2026) rendered as `□`: the bundled LVGL Montserrat
subset has neither codepoint (`°` U+00B0 is present). Only three *rendered* strings used them — the
Live tile placeholder, `Formatter.formatGasIaq`'s fallback, and History's `"Connecting…"`; the rest
are in Javadoc/comments, which never render. Replaced those three with ASCII (`--`, `Connecting...`).
(Adding the glyphs to the font subset was the alternative but costs flash on this heap-tight board and
needs the font toolchain — not worth it for two characters.)

## 5. 🟡 Low — EditText clears its value on edit; QWERTY for a numeric field — FIXED

The field never actually cleared — tracing showed its text became `"30\n"`. The `EditText` SDK is
documented as *"Single-line text input,"* but `create()` never called `lv_textarea_set_one_line`, so
the textarea was multi-line; the keypad **X** (= ENTER) that opens the keyboard *also inserts a
newline*, moving the cursor to an empty second line so the field looks blank (and `parseOr("30\n")`
falls back). Fixed by honoring the documented contract: `edit_text::create` now sets one-line, so
ENTER no longer inserts and "30" stays put.

For the QWERTY-on-numeric half, added Android-style input types: a `picodroid.text.InputType`
(`TYPE_CLASS_NUMBER`), `EditText.setInputType(int)`, a per-field numeric flag, and `show_system_for`
now picks `LV_KEYBOARD_MODE_NUMBER` vs the text layout for the field it binds. `SettingsActivity`
marks its three integer fields numeric. **Verified:** the Temp Hi field keeps "30" on edit and the
soft keyboard opens as a digit pad (1/2/3/…).

## 6. 🟡 Low — Settings hint bar clipped — FIXED

The Settings legend `"A:Up  B:Down  X:Edit/Save  Y:Back"` overflowed the 224 px `ButtonHintBar` and
"Y:Back" clipped to "Y:B" (the other screens use a single-word X hint like "X:Open"). Shortened it to
`"A:Up  B:Down  X:Edit  Y:Back"` — the same length as the others, so the whole legend fits. The Save
button is self-labelled, so dropping "/Save" loses nothing. **Verified:** "Y:Back" now renders fully.

## 7. ⚪ Nit — Home highlight color inconsistency — FIXED

The Home menu's focused row was teal (`colorPrimary`) on first render but blue after any navigation.
The ListView row only overrode the highlight for `LV_STATE_FOCUSED`; keypad navigation also adds
`LV_STATE_FOCUS_KEY`, which the default theme paints blue, so it took over once the user moved. Now
the row sets the teal fill for **both** `LV_STATE_FOCUSED` and `LV_STATE_FOCUS_KEY` (new FFI constant),
so the highlight stays teal throughout. **Verified:** teal on first render and after navigating.

---

## What works

- **Home hub:** A/B move the highlight, wrap-around (Settings↓→Live, Live↑→Settings), X opens each
  destination, Y exits the app (Android launcher behavior).
- **Live (first nav):** all 5 tiles populate with live values (e.g. Temp 22.07C, Humidity 45.04 %,
  Pressure 1013.27 hPa, 205 IAQ, 301 lx).
- **Settings (first nav):** focus traversal with A/B across all controls; X opens the keyboard; Y
  dismisses the keyboard via the back-chain (staying on screen); the Switch toggles visually; Save →
  "Saved" Toast → `finish()` → Home.
- **Back navigation:** returns to the parent Activity and correctly unbinds + destroys the bound
  service (when not in the post-GC broken state).

## Reproduction / methodology notes

- Drive input headless via the control FIFO printed by `sim-remote.sh`, e.g.
  `echo 'tap B' > /tmp/picodroid-sim-remote-<display>-ctrl` (verbs `tap|down|up|press`, keys
  `A|B|X|Y|PREV|NEXT|ENTER|ESC`). The `scripts/sim-ctrl.sh` wrapper does the same without
  the FIFO path: `./scripts/sim-ctrl.sh tap B` (auto-discovers the running sim's display).
- Capture frames with `DISPLAY=:<n> scrot --window "$(xdotool search --name picodroid)" out.png`.
- The app's `println!` output and lifecycle logs land in the `sim-remote` log
  (`/tmp/sim-remote.log` when launched as shown above).
- **Cleanup gotcha:** kill the sim by exact process name (`pkill -x picodroid`). Do **not**
  `pkill -f sim-remote.sh` — that pattern also matches the launching shell and self-kills it.

## Related

- Fix tracked in this repo's GC-root work; see memory
  `project_switch_gc_root_gap_nosuchmethod` and the prior
  `project_gc_collects_unfielded_callback_views`.

---

# Update 2026-08-16 — WiFi showcase (v0.12 networking)

picoenvmon gained a networking feature set on the new **`pico_enviro_mon_w`** board
(Enviro+ Pack on a Pico 2 W; the plain `pico_enviro_mon` board is unchanged and the
app degrades gracefully there — the Network screen shows "WiFi not available"):

- **Live web dashboard** — the device serves `http://<ip>:8080/` (HTTP/1.0, one
  connection at a time, 2 s auto-refresh) with the five smoothed readings, outdoor
  weather, clock, IP and uptime. Serial serving is the architecture: the native
  listen backlog is 1.
- **NTP wall clock** — SNTP against pool.ntp.org anchors
  `System.currentTimeMillis`; History rows show `HH:MM`, the sample dialog shows
  the full date, ALERT log lines carry `[HH:MM:SS]`. All times UTC
  (`TimeFormat.UTC_OFFSET_MINUTES` to shift display).
- **Weather** — one-line wttr.in fetch (plain HTTP), strictly fail-soft
  ("unavailable" on any failure); city is a constant in `WeatherFetcher`.
- **Network screen** — 4th hub entry: status, IP, URL, time, weather + a Refresh
  button (X). A/B/X/Y model unchanged.

Everything networked runs on ONE background thread owned by `NetworkManager`
(app-scoped by design — Android would use a Service; the 16 KiB-per-thread cost and
heap budget favor a single thread whose accept timeout doubles as the housekeeping
tick). All UI updates cross to the main thread via `Executors.mainExecutor()`.

QA hooks: `./scripts/sim-run.sh --app picoenvmon` runs both board smokes (the -w
lane curls the dashboard; NTP/weather assertions accept the fail-soft tokens so
nightly never depends on the internet). Heap pre-flight for the W board:
`./scripts/sim.sh -b pico_enviro_mon_w -a picoenvmon -l 360` under a curl loop.

## Open (P1): device-only panic under combined stress — 2026-08-16

~4 min into an on-device soak (Live screen active with Logger on, dashboard
serving at 2 s cadence, pdb input churn), core 0 panicked:

```
panicked at core/src/slice/index.rs:1020:51:
range end index 388 out of range for slice of length 336
```

Immediately preceded by routine `bme:` sampler debug lines. All demo paths had
verified before it (join/DHCP, dashboard 15/15 paced fetches, NTP sync,
weather, timestamped ALERTs, History via pdb nav).

Known so far:
- **Sim does not reproduce** the identical scenario (Live + Logger + nav churn
  + 45-100 paced requests, twice) — this is in the sim-invisible class.
- Socket send/recv natives are exonerated: both copy through a stack buffer
  around the blocking call, no arena slice held across a yield.
- Suspect surface: an arena/table span inconsistency (a 336-element backing
  store addressed to 388) — plausibly a latent compaction path now exercised
  ~10x more often by the GC pacing fix (native allocs counted, threshold 64
  on this board), or a sampler-task/JVM-heap interaction at real priorities.
- Recipe: `docs/memory-diagnostics.md` offensive mode + the gdb-multiarch
  probe-rs flow in project memory (`reference_gdb_sim_debugging`,
  `project_handle_dangle_sim_blind`); a `--mem-diag` reflash with
  `PICODROID_MEMDIAG_OFFENSIVE=1` should catch the write at damage time.

### 2026-08-17 overnight soak: reproduced as child-thread death + GC thrash

Ran the `docs/picoenvmon-soak-plan-2026-08.md` phases overnight (offensive
mem-diag release build, commit `beb0e3d` debug logs, verified pdb nav; full
artifacts in `build/soak-2026-08-16/`). The corruption class reproduced on
the **first** nav cycle, with a much tighter recipe than the original ~4 min:

1. Boot; dashboard serves cleanly for 15 min (smoke, HTTP-only load).
   `memmon` fired `LEAK? native floor rose` 8x during this window (floor
   243k -> ~282k) then never again.
2. 21:13:43 first pdb nav cycle: X -> `activity: push LiveActivity` (OK).
   Two paced GCs fire during Live's construction (`w=938/939 gc=+1`).
3. The network thread had just run its 15-min weather refresh (last child
   log line: `weather: Clear +58 F`).
4. Mid-dwell on Live: `Thread.start: child-task picoenvmon/net/
   NetworkManager.run() failed: InvalidReference` — the serve thread died
   (no respawn; dashboard dark from here on, 16,473/16,940 fetches 0-byte).
5. The Y-back pop still worked, then the JVM entered a **permanent GC
   thrash**: `GC-PRESSURE 24–27 GCs per window with alloc=+0 nalloc=+0`,
   35,963 warning windows over ~10 h, `live` collapsed 26k -> 5,708 (the
   dead child's population swept). Every later JVM entry is broken: hub
   item clicks never push (399/399 verified X-presses missed), sensor
   delivery logs `sensors: deliver_event err` continuously. Native/LVGL
   stays healthy: key dispatch verified working all night (1,001 PASS),
   pdb ping OK, no reboot, zero OOM, `nmin` 126,768 / `lblk` 108,496 flat.

Reading: a main-thread GC during Activity churn invalidated state held by
the parked/blocked network thread (weather refresh in flight) — same family
as the parked-frames registry fix in `project_jvm_concurrency_gc_fixes`,
which this evidently does not fully cover. The offensive poison trap never
fired, so the damaged reference is not a poisoned freed span (points at
slot-reuse or an unregistered root rather than UAF of heap chunks). The
post-mortem GC thrash (emergency GC re-fired every window at zero alloc)
is a secondary defect worth its own look: a dead child leaves the pacing
state permanently tripped.

Consequences for the other soak objectives:
- **Heap-gate part B: not measurable** — the dashboard was dark from 21:14,
  so the combined-load profile never sustained. (For what it is worth, the
  degenerate 10 h churn held: zero OOM, floors far above the gate.)
- **PEM-3 prereserve retune: not collected** — quiet-hold `memmon storage`
  reflects a JVM without its network thread; re-run after the fix.

Repro recipe (fast, ~16 min): flash offensive mem-diag release; let the
dashboard serve ~15 min so the weather refresh lands; run one pdb nav cycle
(open Live during/just after the refresh window). Debug next via the
gdb-multiarch + probe-rs attach flow against the still-running thrash state
(the zombie survives indefinitely; `pdb ping` works).

### 2026-08-17 gdb post-mortem on the live zombie — mechanism proven

Attached probe-rs gdb + gdb-multiarch to the zombie ~17 h after death (no
reflash). Findings, each read from device memory:

1. **GC-thrash trigger identified.** Breakpoint on
   `SharedJvmHeap::collect_now` fires from `drain_sensor_events`
   (`sensors/mod.rs:541`) — the emergency-GC + single-redeliver arm for
   `JvmError::StackOverflow`.
2. **The failing call is `set_field`, and the error is mislabeled.**
   `deliver_event` does `set_field(event_obj, TIMESTAMP, ..).ok_or(StackOverflow)`
   (`sensors/mod.rs:577-579`). `set_field` returns `None` for a *stale ref*
   (slot `None`), not just alloc failure — so a swept recycled event
   masquerades as allocation pressure and triggers a pointless GC per
   delivery, forever. `Frame::new` never fires (verified): no invoke ever
   starts; hub clicks die silently in the same way.
3. **Native state is intact; the heap slots are gone.** The sensor `STATE`
   static (0x2000b014) still holds 5 registrations
   (listener=10, events 40/42/44/46/48, values 26-30). Dumping the object
   table (`boot::SHARED_HEAP` static, 0x20004b10, chunk 0) shows slot 10
   (listener, Java-reachable) alive — while slots 38-41 (recycled
   events/sensors region, native-only-rooted via `sensors::visit_gc_roots`)
   are all `None` with stale 6-field SensorEvent payloads still legible in
   the slot memory. **A collection swept objects that a registered GC root
   visitor roots** — while Java-reachable neighbors survived.
4. **Concurrency contract audit.** All JVM tasks (main + JvmChild + bg pool
   + sampler) are core-0-pinned (`glue.rs` both arms, verified) — no SMP
   cross-core race. Main jvm_task = FreeRTOS 15; NetworkManager uses
   `new Thread(this).start()` = default Android 5 → also 15; equal priority
   + `TIME_SLICING=0` means the child cannot preempt a running collector.
   The `GcState::parked_frames` safety comment (`gc/mod.rs:74-80`) assumes
   exactly this — so the fatal sweep is NOT simple preemption.

Open first-cause candidates (narrowed): (a) a u16 JVM ref held in *Rust*
native state across a blocking call in the NTP/weather path — invisible to
both the parked-frames registry and `gc_visit_roots`, swept mid-block, slot
reused, written on resume (the send/recv arena-slice exoneration never
covered u16 refs); (b) a child-executor GC (invisible to memmon, handover
§3) running during a main-task yield with a root-set/mark defect. Both are
the `project_native_alloc_gc_gap` family.

**Next step (designed, not yet built):** a mem-diag "root-audit" mode — after
every sweep, re-walk every registered native root and panic at the first
rooted-but-swept object, naming the collector (main vs child executor) and
the root. With the 16-min repro this converts the remaining unknown into a
caught-in-the-act trap. Fix the `set_field` error conflation
(`sensors/mod.rs:579` — distinguish stale-ref from alloc-failure so the
emergency-GC arm stops thrashing) and the thrash's GC-pacing latch at the
same time.

### 2026-08-17 (later): panic caught in gdb — faulting frame named

The 16-min recipe reproduced the ORIGINAL P0 panic shape on the next run
(`range end index 170 out of range for slice of length 166`). A gdb
hardware breakpoint parked on `rust_begin_unwind` caught it with the full
stack intact:

```
copy_within<Value> (dest=166)
ObjectHeap::compact_fields_arena   jvm/src/object_heap/mod.rs:710
gc::collect                        jvm/src/gc/mod.rs:428
interpreter::execute_frames        (the paced safepoint GC)
Jvm::invoke_instance
native_handler::os::dispatch       os.rs:94  <- Thread.start child body
```

The panic is the **network child's own safepoint GC** compacting the shared
fields arena and finding a live object whose span (end 170) extends past
the arena (len 166). Sequentially this is impossible: spans are only
created at the tail (`alloc_span`) and only compaction truncates, so every
live span is in-bounds by construction. The inconsistency therefore
requires the alloc/compact sequence to have been interleaved — yet every
JVM task (main jvm_task, NetworkManager child, bg pool when active) is
core-0-pinned at FreeRTOS 15 with time slicing off, the sampler task holds
no JVM-heap access (audited), and no >15-priority task or ISR touches the
heap (audited). Something in that serialization argument is false in
practice.

Instrumentation added (all offensive-gated, commit pending): span-invariant
checks (`ObjectHeap::debug_check_spans`) at gc-entry / post-compact /
post-alloc / post-lazy-grow — panics at the moment the inconsistency is
CREATED, with the victim's call site in the backtrace, disambiguating:
damage-before-GC vs created-by-compact vs truncation-raced-an-alloc. Plus
the post-sweep root audit in `gc::collect` (re-walks all root sources;
firing = mark/sweep state disturbed mid-collection). Plus two fixes landed
on evidence already in hand: `deliver_event` stale-ref guard + drain
unregister arm (kills the eternal thrash), and the **cross-executor handler
root registry** (`HandlerRootGuard`): child-executor GCs previously rooted
only their own handler's Activity stack/pending ops, so a child GC while
the main task idled between executes could sweep the UI's Activities — a
confirmed hole regardless of whether it is this crash's first cause.

### 2026-08-17 (later still): two more repro rounds — the mechanism takes shape

**Round 2** (span traps built, ~16 min): panicked INSIDE
`compact_fields_arena` again but with the OTHER failure mode — `dest is out
of bounds` (the compaction write cursor overran the arena). Bounds-clean
individual spans plus an overrunning write cursor = the live spans
**overlap** (sum of caps > arena len). An interleaved `alloc_span` pair
produces exactly both observed panic shapes at once: two contexts read the
same `fields_arena.len()`, both take the same offset (overlap ⇒ round-2
panic), and the loser's smaller `resize` truncates the winner's span
(⇒ round-1's `end 170 > len 166`). Sequentially impossible; some context is
interleaving the alloc.

**Tooling root cause found while wiring round 3:**
`PICODROID_MEMDIAG_OFFENSIVE` was read ONLY by the sim
(`docs/memory-diagnostics.md` even says "sim only") — **the device never
armed offensive mode**. Every on-device "offensive" conclusion to date is
void: the overnight soak's poison-trap silence meant nothing, and round 2's
span traps were inert (so "gc-entry passed" was never actually tested).
Fixed: `mem_diag::apply_device_flags()` bakes the flag at build time
(`option_env!`), logs `memmon: offensive checks ON (build-baked)` at boot,
and installs a task-id hook (`rtos::task_current`) so the new offensive
alloc-trace ring `(task, offset, n_fields)` names which tasks' allocations
interleaved. `debug_check_spans` now also runs the full overlap sweep
(`integrity_check`) at all four contexts.

**Round 3** (first genuinely-armed offensive run): panicked in
`compact_fields_arena` (`dest is out of bounds`) with gc-entry and every
post-alloc check PASSING — the corruption forms between checks. Post-mortem
of the halted device delivered the root cause:

- Panicking task (core 0 current, `pxCurrentTCBs[0]`): TCB name
  `picoenvmon/net/` — the NetworkManager child, mid-its-own safepoint GC.
  Core 1: IDLE. Alloc-trace ring: all 8 recent spans from the child
  (1-field Socket-sized allocs at the arena tail, offsets REPEATING —
  truncations between allocs).
- Object-table dump at death: a chain of overlapping tail spans at offsets
  162/165/168/170/171 — each allocation landing 2-3 slots BEHIND the
  previous span's end. Classes (read from the live class table):
  `Sensor`/`SensorEvent` (main task: SensorLoggerService re-registration on
  every History nav cycle) alternating with
  `HttpServer`/`ServerSocket`/`Socket` (network child: accept loop). Two
  interleaved allocators, repeatedly, within seconds.

**ROOT CAUSE (kernel-source confirmed):** the FreeRTOS SMP kernel's
`prvYieldForTask` (third_party/FreeRTOS-Kernel/tasks.c:910) yields when an
unblocked task's priority is `>=` the running task's — equal-priority WAKE
preemption, which `configUSE_TIME_SLICING=0` does not disable (that only
stops tick round-robin). The global allocator's `xTaskResumeAll` exit is a
yield point, and `alloc_span`'s `try_reserve_exact`/`resize` (and the GC's
scratch-Vec growth) allocate — so a socket completion readied by core 1's
IP task (or an expiring UI delay) preempts a JVM task MID-COMPOUND-HEAP-
OPERATION despite core pinning, equal priorities, and no slicing. Two tasks
then read the same arena length in `alloc_span`; the loser's `resize`
shrinks the arena over the winner's fresh span → overlapping/orphaned
descriptors → compaction range/dest panics (both observed shapes), and the
same yield inside a GC's Vec growth lets the other task mutate the heap
mid-collection → rooted-objects swept (the InvalidReference child death +
sensor-event sweep + GC thrash). One mechanism, every observed failure.
This falsifies the single-core serialization contract documented on
`GcState::parked_frames` and assumed throughout.

**FIX (landed with this session's commits):** `pico_jvm::atomic_section` —
platform-installed scheduler suspend/resume hooks
(`vTaskSuspendAll`/`xTaskResumeAll`, installed in `boot_tasks::start_tasks`;
nests safely with heap_4's internal suspension so the inner resume never
yields) wrapped as an RAII guard around every compound heap mutation:
`gc::collect` (whole collection), `ObjectHeap::alloc_with_field_count`,
`set_field` (lazy-grow), `ArrayHeap::alloc`, `StringTable::intern` /
`intern_dyn_owned`. No-op on host/sim (no hooks installed). Guarded
sections never block. This restores the parked_frames safety contract by
construction rather than by scheduling assumption.

**Validation (fix commit `0c1326d`):** the 16-min combined-load repro — a
3/3 kill rate across the previous rounds — ran CLEAN with every offensive
trap armed: 14 full nav cycles through the weather-refresh window, 195
verified presses, zero panics / span / integrity / root-audit fires, zero
swept registrations, dashboard and pdb healthy after the window. (The
driver's 28 "FAIL key=23/4 no-dispatch-log" lines are all the Settings
NumberPicker edit-mode filter consuming X/Y natively before the Java queue
— known drop-point, benign; every cycle's subsequent pop verified 14/14.)
Full pre-commit and the sim smoke trio pass. Remaining follow-ups: the
heap-gate part B soak + PEM-3 prereserve retune can now actually run;
consider an edit-mode key log to make even that native consumption
observable; audit the sim's Thread.start parallelism for the same race
class (host threads have real parallelism and no atomic-section hooks);
sb_buf cross-thread aliasing (handover §7) remains its own item.
