# Platform gaps found building `claudeusage`

**Status: open list; G1, G2 and G3 closed 2026-09-23, G8 closed 2026-09-24, D5 (app bug) fixed 2026-09-23, D2 fixed 2026-09-25 (`LV_DRAW_SW_SUPPORT_RGB888`, the horizontal gradient's source format). D4 closed for this app 2026-09-25: the RP2350's flash clock was the ROM's divider of 3 and is now 2 (`hal/rp/xip.rs`, every RP2350 board), the History and Models first paints are split, and no page turn has a span over 50 ms; the one runtime follow-up left, a RAM-resident interpreter loop, is measured below and waits on a decision about the 30 KB. G11 attributed 2026-09-24: the board has 98 KB free on its worst page, the simulator 9 KB; the levers are listed there. G10 is next.**

`examples/claudeusage` is a desk display for Claude usage limits on a new board, `pico_display2_w`
(Pimoroni Pico Display Pack 2.0 on a Pico 2 W). It was built to look like a modern product rather
than a demo, modelled on the ESP32 usage monitors people have published (Clawdmeter, TokenMeter,
claude-code-usage-monitor, ohmyclawd). This file records every place picodroid made that harder than
it should be, what the app does instead, and what closing the gap would take.

The app works around all of these; none blocks it. They are ordered by how much they cost a
polished UI, with the three defects found during simulator QA first because they are bugs rather than
missing features.

Everything below was observed on the simulator (`./scripts/sim.sh --board pico_display2_w`)
unless it says hardware.

## Defects

### D4. Every tick of a page swap overruns the slow-handler budget on the RP2350 — closed 2026-09-25

**Status: closed for this app (2026-09-25); see "The flash clock" at the end of this section for what did it and what is left for the runtime.** Before that: root-caused and mostly fixed (2026-09-23/24); see "Findings" and "Follow-ups landed" below. Candidates 0
to 2 are answered: the pre-round app is just as slow on today's runtime, nothing preempts the
UI task, and the cost is per-bytecode interpreter overhead plus LVGL object creation, not
the app. The runtime fixes landed (persistent set-associative resolution tables, frame
buffer pooling) take the small steps under the line; the big steps (a Models or Burn build
step with 200–270 invokes and 320–360 field ops) stay at 80–140 ms and need either fewer
objects per step or the follow-ups listed under the findings.

**Status before that: open (2026-09-23).** Added after the hardware run of the Android-shape round
(`claudeusage-android-shape-2026-09.md`, commit `79369105`). Listed first because every button
press stalls input and the 1 Hz ticker for 50 to 175 ms across roughly ten consecutive ticks, and
because two of the three candidate causes are runtime-wide, not this app's.

**Measured** (debug firmware, `pico_display2_w`, live bridge, keys injected with
`./scripts/pdb.sh --board pico_display2_w input keyevent 20`):

- One `slow handler: Runnable took 51..74 ms` per page turn, on the tick right after the swap.
  The warning is rate-limited to one per second (`lifecycle/mod.rs::warn_if_slow`), so one line
  per turn is the floor, not the count.
- With `SystemClock.elapsedRealtimeNanos()` around each posted step of a turn, from inside Java:
  discard 20 to 30 ms; start (construct the page, log, chrome) 46 to 55 ms; each build step (one
  to eight LVGL objects) 19 to 175 ms, typically 50 to 90; finish (first `update`, fade) 60 to
  100 ms. The simulator runs the same steps in single-digit milliseconds.
- Emptying the first tick after the swap down to one `Log.i` (2.5 ms measured inside Java) still
  produced the 51 to 74 ms warning on that tick. GC is not it: `Runtime.gcCount()` did not move
  across a turn and total GC time was 10 ms over four turns.

**Measured 2026-09-23, ring-gauge round (`pico_display2_w`, probes inside `LimitsPage.update`).**
The Limits page's first `update` after a swap costs ~100 ms in both the bar and the ring
versions: 27 to 35 ms per card is `BigNumber` creating its glyph `ImageView`s on first show
(gone since the figures became `TextView`s in `464f1e63`; **not yet re-measured on hardware**),
the bar 6 to 11 ms, the ring 4 to 8 ms, the text lines 5 to 6 ms. With bars the swap tick's own
51 to 74 ms warning fired first and the update's was swallowed by the one-per-second rate limit;
with rings the two land in one tick, so a single `Runnable took 114 ms` shows the whole cost.
Later minute ticks cost 21 to 34 ms for the whole page. A cheap cut would be creating the glyph
slots hidden during the build steps so the first visible tick only sets positions and sources.

**Not the cause: the redraw.** `g.tick()` (render plus SPI flush) runs on the same main task as
its own `MainTask::LvglTick` and is excluded from the timed spans, so it cannot sit inside a
Runnable's span. The note in the Android-shape doc and in commit `79369105` blaming "the redraw
of the swapped region" was wrong and is withdrawn here.

**Candidates, in the order to test:**

0. **A/B against the pre-round app.** `217f071c` reported the old app under budget after its
   own tick-splitting. Flash `d72d6dae^` (`examples/claudeusage` before the Android-shape round)
   and turn pages: if the warnings are gone, the regression is the round's extra classes and
   members, which points at candidate 1.
1. **Declined resolution caches, so every call re-resolves.** The method and field caches
   (`crates/jvm/src/interpreter/helpers.rs::cache_push`) decline to grow when the doubling block
   is refused; on the RP2350 that block is 10,240 B, and the sim soak of this app showed exactly
   that refusal from the third page on (`project_jvm_cache_push_fallible`). Once declined, every
   invocation of a method not already cached walks the class list again, per call, which is the
   shape of "the same Java is ten times slower on device than in the sim, and a nearly empty tick
   costs 50 ms". Test: a defmt counter of declined pushes and cache misses, or a run with a
   larger initial capacity; if the build steps drop, the fix is chunked or bounded caches.
2. **FreeRTOS rotation inflating wall-clock spans.** When a higher-priority task (cyw43 network,
   SPI, pdb) wakes and blocks, the kernel resumes the *next* equal-priority task, not the one it
   interrupted (`jvm_run_lock` design note), so the main task's Runnable waits for the ticker or
   poll thread's slice. Test: `PICODROID_EXTRA_FEATURES=sched-diag ./scripts/flash.sh …`
   (`docs/scheduling-diagnostics.md`) and `./scripts/pdb.sh sysmon` while turning pages, with
   WiFi idle versus mid-fetch.
3. **LVGL object creation itself.** Each `Ui.box`/`label` is one object with a fresh style set,
   each right- or centre-aligned label two; the eight-bar build steps of `BurnPage` cost 90 ms.
   If 1 and 2 are clear, the answer is fewer objects (G3, G4), not finer slicing.

**Findings (2026-09-23, evening).** Measured with the `parity-metrics` span report
(`PICODROID_EXTRA_FEATURES=parity-metrics ./scripts/flash.sh …`): every slow main-loop span now
prints the UI task's CPU time, time inside natives (and the slowest and fastest native by name),
time resolving method and field sites, time in the class-initialised probes, invoke / field /
`new` / other opcode time, frame-allocation time, bytecodes, cold resolutions, and the clock's
own cost. On the simulator `PICODROID_TRACE_SPANS=1` prints every span, slow or not.

- **Not the app.** The pre-round app (`d72d6dae^`, installed over `pdb install` onto today's
  firmware) turns pages in 55–160 ms, the same as the Android-shape app. Release firmware is
  the same as debug (54–123 ms).
- **Not the scheduler.** For a 52 ms Runnable the task's FreeRTOS run-time counter moved
  51.6 ms: nothing preempts or blocks the UI task. `sysmon` agrees (cyw43 0.0 %, Tmr Svc 0.5 %).
- **Not the caches declining.** `declines=0` on every span; the tables never refused.
- **Where a 60 ms step (847 bytecodes, 96 invokes, 36 natives) went, before the fixes:**
  natives 13 ms (a fixed floor of ~60 µs per native call through the string-matched
  dispatcher chain, and `LinearLayout.nativeCreate` at 1.6–2.0 ms each — `lv_obj_create` on
  the screen plus six `lv_obj_set_style_pad_*`, each a style refresh); cold resolution 15 ms
  (109 sites at ~140 µs: the executor's memo tables lived only for one top-level invocation, so
  every posted Runnable started cold and walked `TextView → View → Object` comparing ~200
  method names read from XIP flash); class-initialised probes 0.5–2 ms (a linear name scan on
  every `getstatic`/`invokestatic`/`new`); frame allocation 4 ms (two heap reservations per
  Java call, ~65 µs, heap_4's first-fit walk); and ~25 ms of plain interpretation at 15–35 µs
  per field or invoke bytecode — about ten times the benchmark app's 3 µs, which is the XIP
  instruction-cache cost of the interpreter's sprawling opcode handlers on UI code, and is not
  addressed here.
- **Fixed in the runtime** (`crates/jvm/src/resolve_cache.rs`, `frame.rs::FramePool`): the
  method, field, static and `new` sites resolve into four-way set-associative tables that live
  on the shared heap beside the Class-object cache, persist across Runnables and threads, are
  allocated once at a fixed size (16 KB on the RP2350, 4.3 KB on the RP2040 — no doubling, so
  the 10,240-byte request of `project_jvm_cache_push_fallible` is gone), and carry the
  class-initialised flag so the probe runs once per site. Frames draw their two buffers from
  an executor-local pool. `find_method` reads a descriptor only after the name matched (half
  the flash reads on a miss). On the board: cold resolutions per step 109 → 0–13 on a warm
  lap, clinit probes ~0, frame time halved; the ~850-bytecode steps went from 55–60 ms to
  under the 50 ms line, and the big steps lost 15–20 %: Models 139 → 119–126 ms (natives
  37 ms/112 calls, invokes 74 ms/270, fields 14 ms/359 ops), Burn 105 → 85 ms, History
  92 → 77 ms.
- **Left for follow-ups**, largest first: (a) `nativeCreate` at 1.7 ms — try
  `lv_obj_enable_style_refresh(false)` around the create sequence, and creating under the
  final parent instead of the screen; (b) the interpreter's per-bytecode cost from XIP — a
  RAM-resident core loop, or fewer, fatter opcodes; (c) the ~60 µs native-dispatch floor — a
  pointer-keyed memo from class name to sub-dispatcher; (d) at the app level, halve the
  objects per build step (the Models and Burn pages).
- **On the simulator** the same steps are 0–1 ms (misses cost microseconds there), which is
  why none of this showed before the board did.

**Follow-ups landed (2026-09-24).** Same board, same `parity-metrics` debug build, two laps of
page turns before and after; the parity build prints every slow span, so a page with no line
has every step under 50 ms.

- **(a) `nativeCreate`: batched style refreshes.** LVGL's automatic style refresh is switched
  off around the style sets of the `LinearLayout`, `ScrollView` and `NumberPicker` creates,
  `View.setPadding` and `GradientDrawable.applyTo`, with one refresh at the end
  (`graphics/lvgl/style_batch.rs`; the refresh carries a representative property's flags, not
  `LV_STYLE_PROP_ANY`, which would also walk every descendant). `LinearLayout.nativeCreate`
  1.65–1.95 ms → 1.35–1.59 ms: the six refreshes were a fifth of it. The rest is
  `lv_obj_create` itself — theme, init, the eleven `lv_style_set_prop` reallocs — every
  instruction of it fetched from XIP, and it stays. *Creating under the final parent* is not
  possible as such: `nativeCreate` is static and the parent only arrives with `addView`. The
  nearest thing, a hidden holder object, would spare only the create-time invalidations while
  changing what an unattached view does (today it is drawn on the screen), so it was not done.
- **(c) The dispatch floor: a per-site memo.** The handler remembers, per `(class, method)`
  name pair, which sub-dispatcher claimed it or that none did
  (`native_handler/dispatch_memo.rs`, 64 rows, keyed by the names' addresses, emptied when a
  new app run starts). Fastest native per step 50–70 µs → 27–35 µs. What remains is the
  interpreter's invoke path, the graphics module's own class and method match, the handle
  lookups and the parity clock.
- **(d) App: fewer objects per step.** Burn builds four trend bars per step instead of eight
  and its two right-aligned lines in a step of their own; Models builds one meter row per step
  instead of three. Burn: 80–90 ms per step → no step over 50. Models: 121–128 → no row step
  over 50; the title step is 53 ms and the first `update` 78 ms (the page's own `setText` and
  tint sets, 70 natives — `TextView.setText` at 0.6–0.7 ms is now the slowest native).
- **Unchanged:** History (61–88 ms per step) and the Limits step (52 ms), not in this round;
  History wants the same split. **(b) remains:** interpreting from XIP, and with it the
  observation above that *all* native code pays the same fetch cost — the create's residual
  1.4 ms is a few thousand LVGL instructions at flash speed, not any one expensive call.

**The flash clock (2026-09-25).** Same board, same `parity-metrics` debug build, laps of eight
page turns. The morning's baseline: Limits first update 56–62 ms, Models first update 72–77,
History bar step 72 and first update 86–87; Burn clean. The counters said where the time was
not: 383 field ops cost 14–15 ms (38 µs each) and 24 cold resolves 9–10 ms, on a hit path that
is a hash and four compares in RAM. Everything on this board runs at the speed its code can be
fetched from flash, so the fetch itself was measured:

- **The QSPI clock was the ROM's.** The image has no boot stage 2, so XIP window 0 ran in the
  mode the RP2350 boot ROM discovers — quad-I/O `EB` reads, 8-bit command prefix per burst,
  `M0_TIMING` divider 3 (50 MHz SCK at 150 MHz clk_sys). The pico-sdk's boot2 programs
  divider 2 and RXDELAY 2 for every RP2350 board (`boot2_w25q080.S`). `hal/rp/xip.rs` now
  retimes the window to those figures once after `clock_init`, guarded on the ROM having
  picked `EB` (the serial `03h` fallback is rated to 50 MHz). A 128 KB read through the
  uncached alias: 6.2 ms → 4.7 ms (1.33×). Every span 20–25 % shorter: Limits under the line,
  Models 57–59, History 54 and 65–66. `with_xip_disabled!` restores the retimed registers
  after a runtime flash write; two AUTO toggles (preference writes) left the spans unchanged.
  The boot log line `[xip] window 0: quad EB, clkdiv 3 -> 2, …; 128 KB uncached read N us`
  is the figure to watch: a restore that ever falls back to `03h` shows as a 14× N.
- **`opt-level = "s"` is slower, not faster.** Tried for the whole workspace: flash 1,539 →
  1,424 KB, but every span 8–15 % longer (History 54 → 60, 66 → 73). At `s` the opcode handlers
  stop inlining into `Executor::run` (29.6 → 3.1 KB, the handlers 2.7–5.1 KB each), so a
  bytecode crosses more call boundaries, each a fresh set of cold lines. Reverted; `3` stays.
- **A RAM-resident interpreter loop works, and costs 30 KB.** `#[link_section = ".data"]` on
  `Executor::run` (29.6 KB at `opt-level = 3`, 35.6 KB with the release profile's fat LTO)
  with the arena cut 408 → 372 KB to fund it: Models first update gone, History bar step gone,
  History first update 65 → 51–52; `other` (the plain opcodes) 6.5 → 2.0 ms and fields
  11.8 → 9.2 ms per History paint. What is left in a 51 ms paint is natives 12.5 ms, cache
  probes 7 ms and the invoke path's own 14 ms (constant-pool reads of names and descriptors
  from flash, `count_args` over the descriptor, the name compares before dispatch), all of it
  outside `run`. Not landed: the 30 KB comes out of the arena on every RP2350 board, which is
  a product decision (opt-in per board through `[jvm]` in board.toml and the JVM's
  `build.rs`, funded by H8 + H9 + H7 at 23 KB, would leave ~7 KB of headroom on this board);
  the RAM copy could also be halved by keeping the cold handlers (math, convert, arrays, indy,
  monitors, the exception and GC tails) out of line in flash. XIP-cache pinning (op 7 in the
  maintenance alias, 8-byte lines) is the no-RAM alternative, at the cost of one of the two
  cache ways for the pinned sets; not tried.
- **App, on top of the clock.** History builds four bars per step (`BARS_PER_STEP`, as Burn)
  and builds them in their final colour and radius while the page is still invisible, so the
  first paint only sizes them and re-fills only a day without tokens (`shownFill`): no History
  span over 50 ms in three laps. Models paints one card per tick on the first paint
  (`Page.paintNext`, driven by `MainActivity.paintPage` before the fade-in; later updates stay
  whole), and `BarView` skips `setProgress` for an unchanged value, which the cleared rows were
  paying as a native each: no Models span over 50 ms either. Twelve turns and two preference writes on the final build:
  no slow span at all. What remains is one-off: the Service start (`pending-op drain`, ~60–75 ms)
  and the first page after boot (65 ms, 83 cold resolutions), before the tables are warm.

**Repro.** `env $(grep -v '^#' .wifi-creds.env | xargs) PICODROID_NET_TEST_HOST=<PC address>
./scripts/flash.sh --board pico_display2_w --app claudeusage` in the background, wait for
`sync ok`, then `./scripts/pdb.sh --board pico_display2_w input keyevent 20` and watch the RTT log.

### D5. App: a sync pressed while offline spun the poll loop — fixed 2026-09-23

An app bug, not a platform one, recorded here because the symptom looks like a platform fault.
`UsageService.refreshNow()` sets `refreshRequested`, but only the connected branch of `pollLoop()`
cleared it. Pressing X with the link down left it set, so `idle()` returned at once on every pass
and the poll thread posted `applyLinkOnly` in a tight loop: the 64-slot main queue stayed full and
`MainExecutor.execute: queue full, dropped` fired 4,268 times in 8 s (`pico_display2_w` built
without WiFi credentials). Fixed in `7d2083c8` by clearing the flag at the top of every pass;
three X presses offline now give zero warnings. Two things worth knowing from it: the main queue
is bounded and drops with only a log line (Android's `Handler` queue is unbounded), and a
runaway poster shows up as that warning, not as a hang.

### D1. Sim: a connect timeout to an unreachable host fires late, sometimes very late

**Repro.** Build the app against an address nothing answers on, run it with no bridge:

```bash
PICODROID_NET_TEST_HOST=10.255.255.1 ./scripts/build-apk.sh --app claudeusage
./scripts/sim.sh --board pico_display2_w --apk build/apks/claudeusage.papk
```

The poll thread calls `HttpURLConnection.connect()` with a 4 s connect timeout, then retries every
15 s, so `[ClaudeUsage] fetch: timeout: connect timed out` should appear at about 5 s and 24 s.
Of six runs checked at 26 s, one showed both lines, three showed only the first, and two showed
none. In a later run with none at 12 s, the first timeout was watched for and arrived between 16 s
and 20 s, four to five times the requested 4 s. Whether the worst cases ever return was not
measured.

While a connect is overdue the rest of the app is healthy: the main thread dispatches keys, and a
second Java thread that only sleeps and posts to the main executor keeps its 1 Hz cadence. The
stalled thread shows in `/proc/<pid>/task/*/wchan` as `futex_do_wait`, not `poll`, which suggests it
is suspended by the FreeRTOS POSIX port rather than sitting in the syscall; that reading was not
confirmed (ptrace is restricted on the dev host, so no backtrace was taken).

A refused connection (`127.0.0.1`, no listener) returns at once and never shows this. The code is
`TcpStream::connect_timeout` in `crates/picodroid-core/src/hal/sim/net.rs::tcp_connect`, whose
comment relies on std tracking the deadline across EINTR. The receive paths in the same file had
to carry their own `Instant` deadline because the 1 kHz SIGALRM tick defeats kernel timeouts; the
connect path may need the same treatment (non-blocking connect plus a deadline-tracked poll).

**Why it matters.** "The PC is switched off" is this app's main failure mode, and the sim
misreports how long each attempt blocks.

**Hardware (2026-09-21, `pico_display2_w`): not affected.** With the bridge address set to an
unused address on the same subnet, every attempt failed promptly with `NoRouteToHostException`
(ARP gets no answer), the app showed `PC offline`, and six retries held the 15 s cadence to within
the 3 s sampling. An address behind a router, where the SYN is simply dropped and the 4 s connect
timeout itself has to fire, was not tried.

**App-side mitigation (done, verified in the stalled runs).** UI ticks come from their own thread
rather than the poll thread, and a fetch still running after 12 s is presented as `PC offline`, so
a slow or stuck fetch cannot leave live-looking numbers or a frozen countdown on screen.

### D2. `GradientDrawable` LEFT_RIGHT gradients do not render — closed 2026-09-25

**Status: fixed 2026-09-25.** The swapped-RGB565 blender was the right place to look, but the
cause was configuration, not the blender. LVGL's software fill draws a horizontal gradient by
blending its gradient map as an RGB888 *source image* (`lv_draw_sw_fill.c` sets
`src_color_format = LV_COLOR_FORMAT_RGB888`; the map is an `lv_color_t` array), whereas a vertical
gradient is a solid fill per line and never goes through an image blend. `lv_conf.h` had
`LV_DRAW_SW_SUPPORT_RGB888 0`, so `lv_draw_sw_blend_image_to_rgb565_swapped` fell into its
`default:` arm, a `LV_LOG_WARN("Not supported source color format")` that draws nothing, and
with `LV_USE_LOG 0` it did so silently. `lv_draw_sw_triangle.c` takes the same path, so any
gradient triangle was lost the same way. The switch is now on, with a comment in `lv_conf.h`
naming both consumers; a `pd-lvgl-sys` test pins it to 1 and checks the vendored fill still hands
the blender RGB888, so a future LVGL bump that changes the source format is a review event;
`examples/displaydemo` has a left-to-right row under the themed header. Flash cost:
+3,732 B on the RP2040 and +3,504 B on the RP2350 for the RGB888 source arm of the linked blenders and the RGB888 image transform path. The switch would also have linked the RGB888 *destination* blender, another 2.8 KB that nothing renders into, so `build_support/lvgl.rs` leaves `lv_draw_sw_blend_to_rgb888.c` out of the compile and `crates/pd-lvgl-sys/lvgl/pd_blend_to_rgb888_stub.c` supplies its two entry points, which assert if ever reached.

`new GradientDrawable().setGradient(a, b, GradientDrawable.Orientation.LEFT_RIGHT)` applied to a
`FrameLayout` draws nothing: the view is fully transparent, with or without a corner radius. The
same call with `TOP_BOTTOM` renders correctly, and so does `setColor`. `examples/displaydemo` only
exercises `TOP_BOTTOM`, which is why nothing caught it. The native side
(`crates/picodroid-core/src/graphics/lvgl/drawable.rs::apply_gradient_drawable`) passes
`LV_GRAD_DIR_HOR` through, so look at the LVGL software renderer's horizontal-gradient path under
`LV_COLOR_FORMAT_RGB565_SWAPPED` first.

The app wanted a left-to-right gradient on its progress bars and uses a vertical one instead. Fix
plus a `displaydemo` row for the horizontal case.

### D3. `View.close()` on a child leaks the subtree

`View.close()` frees the widget but leaves the view in its parent's Java-side `mChildren`, so the
whole closed subtree stays reachable until the parent dies. The app first disposed of pages this
way: live heap went from 14 KB to 48 KB over a dozen page turns, and then a single 38,912 B
allocation (largest free block 25,880 B) threw `OutOfMemoryError` on the main executor.
`ViewGroup.removeView(child)` is the correct call and the app now uses it; the soak is flat.

`close()` is public and looks like the obvious way to dispose of a view. Either make it detach
from the parent, or document on `close()` that a parented view must go through `removeView`. An
offensive check in the sim (a closed view still present in a live parent's child list) would have
named this in one run.

## Gaps, by cost to the UI

### G1. One font size — closed 2026-09-23

Only Montserrat 14 exists and there is no `TextView.setTextSize`. A dashboard needs one figure
readable across a room. The app renders its large percentages from pre-rendered glyph sprites
(`examples/claudeusage/tools/gen_digits.py`, `ui/BigNumber.java`): 13 images, about 30 KB of flash,
fixed colour, fixed background. picoclock solved the same problem with seven-segment rectangles.
Two apps have now built their own numerals.

**Ask:** one or two more sizes, even a digits-and-punctuation subset at 28 and 48 px, plus
`setTextSize`. Subset fonts keep the flash cost to a few KB each.

**Done 2026-09-23** (branch `textview-textsize`): `TextView.setTextSize(float)` /
`(int unit, float)`, `getTextSize()`, `getLineHeight()`, `TypedValue`, `DisplayMetrics`,
`android:textSize`. Not a digits-only subset — a general `setTextSize` must render letters — but
ASCII-only Montserrat faces at 20, 28 and 64 px (`scripts/gen-fonts.sh`), compiled per board from
a `text_sizes` key (the RP2350 default; the RP2040 keeps 14). 64 px draws 44 px digits, the
height of the sprites. A size snaps to the nearest compiled face; `getLineHeight()` says which.
The app's sprites, `BigNumber` and `tools/gen_digits.py` went with the follow-up commit that
put the figures on `TextView`s (`Ui.DISPLAY_SIZE`).

*Follow-up 2026-09-23 (`5f0dd5f5`):* `setIncludeFontPadding(false)` on a single-line label took
the trim off twice (LVGL 9.6 clamps the text height by `max_height` before adding the pads), so
the 64 px face measured 48 px instead of 57 and the Limits percentage ran into its caption. The
single-line cap now adds the frame only when it is positive.

### G2. No arc or ring gauge — closed 2026-09-23

Ring gauges are the signature look of the ESP32 usage monitors, and LVGL has `lv_arc`. There
was no SDK widget for it, and the app used bars throughout.

**Ask:** an arc widget (progress, range, colour, stroke width, start and sweep angles).

**Landed:** `picodroid.widget.CircularProgressIndicator`, Material Components' ring as a
`ProgressBar` subclass over `lv_arc` (colour, stroke, size, direction, corner radius with
Material's names; `startAngle`/`sweepAngle` in `Canvas.drawArc` terms as the one picodroid
extension; inflatable from XML). Range is `ProgressBar`'s `setMax`/`setMin`, which G3 delivered the same day.
The app's Limits page is two 270° gauges side by side, the pace marker a thin overlay arc.

### G3. `ProgressBar` cannot be styled per instance

**Closed 2026-09-23.** `ProgressBar` gained Android's range and tint API — `setMax` / `getMax`,
`setMin` / `getMin`, `setProgress(int, boolean animate)`, `incrementProgressBy`, and
`setProgressTintList` / `setProgressBackgroundTintList` / `setIndeterminateTintList` over a new
single-colour `picodroid.content.res.ColorStateList` — and layouts take `min`, `max`,
`progressTint`, `progressBackgroundTint` and `indeterminateTint`. `ui/BarView` is now one
`ProgressBar` plus the pace-marker tick. Not modelled: state sets in `ColorStateList`,
`setProgressDrawable` (so the bright-to-deep gradient the old bars had is gone: tints are flat),
`setSecondaryProgress`.

Was: determinate bars took their colour from the theme (`setTint` only affected the indeterminate
spinner), the range was fixed at 0..100 with no `setMax`, and there was no animated `setProgress`,
so the app built `ui/BarView` from two `FrameLayout`s.

### G4. No chart and no custom drawing

There is no `Canvas`/`onDraw`, no line, and no chart widget. The app's two charts are one view per
bar (24 and 7), which is the expensive way to draw rectangles: each costs LVGL pool and several
milliseconds to create, which is why pages are built a few views per tick. A sparkline (a line,
not bars) is not expressible at all.

**Ask:** `lv_chart` as a widget, or a minimal `Canvas` with `drawLine` / `drawRect` / `drawArc`.

### G5. Image assets lose their alpha

PAPK assets are RGB565 with alpha discarded. Sprites must be drawn onto the exact colour they will
sit on and cannot be placed over a gradient or a second card colour. The numeral sprites
hard-coded `Palette.CARD`, in one colour, until the figures became `TextView`s (2026-09-23).

**Ask:** an RGB565A8 (or A8-only, tintable) asset format. A8 glyph masks would also cover most of
G1 for apps that ship their own numerals.

### G6. No backlight brightness

The backlight is a GPIO, on or off. An always-on desk display wants dimming at night. `pin_bl` is
PWM-capable on every current board.

**Ask:** PWM backlight with a settings-level brightness API, and an idle *dim* stage before idle
*off*.

### G7. No general build-time app configuration

The only build-time string an app can receive is `NetTestConfig.HOST`, a test hook this app reuses
for its bridge address (port hard-coded). There is no `BuildConfig`, and a four-button device has
no practical way to type an address into `SharedPreferences`.

**Ask:** a `buildConfigField`-style Gradle block generating `BuildConfig` constants from properties
or environment variables. mDNS / DNS-SD would remove the need for an address at all.

*Amendment 2026-09-22:* the app now reads the host from the `bridge_host` key of its `settings`
preferences, with `NetTestConfig.HOST` as the default, so an installed unit can be repointed
without a rebuild (`pdb`, or a future settings screen). The `BuildConfig` ask stands for the
default. See `claudeusage-android-shape-2026-09.md`.

### G8. Keys need a focused widget — closed 2026-09-24

There was no `Activity.onKeyDown`; keys reached Java only through `View.setOnKeyListener` on the
focused view. A screen with nothing to focus (a dashboard) had to add an invisible focusable
`Button` just to receive keys, as this app and `examples/keydemo` both did. There is also no
long-press or repeat for keys, which would have given four buttons eight actions.

**Ask:** `Activity.onKeyDown` / `onKeyUp` as the fallback when no view consumes the event, and
`KeyEvent.getRepeatCount()` / long-press.

**Landed:** `Activity.onKeyDown(int, KeyEvent)` / `onKeyUp(int, KeyEvent)`, reached after the
focused view's `OnKeyListener` declines (or there is none), through the same `final`
trampolines the lifecycle uses (`lifecycle/input.rs::dispatch_key_events`). The defaults carry
Android's BACK contract with `KeyEvent.startTracking()` / `isTracking()`: `onKeyDown` consumes
and tracks BACK, `onKeyUp` runs `onBackPressed` for a tracked release, so consuming BACK's press
is the whole of "never finish". The app's key catcher is gone; `keydemo` shows both paths.
**Still open:** long-press and repeat (`getRepeatCount()`) — one DOWN and one UP per press.

### G9. No TLS

By design (flash budget), and recorded in the parity roadmap. It is why this app needs a PC-side
bridge at all, and therefore why "the PC is off" is a failure mode the app has to design around.
Listed for completeness; the bridge is a reasonable answer for a token that should not live on a
microcontroller anyway.

### G10. A large contiguous allocation under fragmentation

Seen while D3 was leaking: one 38,912 B request failed with 62 KB free because the largest free
block was 25,880 B (`[sim] OOM: tried 38912 B`). The size is consistent with a JVM-side table
doubling as the live object count grew, but the allocation site was not identified. If it is a
doubling table, any app whose live set crosses the same threshold on a fragmented heap hits it,
and chunked growth (as the object and string stores already do) would make it a non-event. First
step: name the site (`reference_device_big_alloc_backtrace` style, or the sim's OOM backtrace).

**Named (2026-09-23):** it is the collector's arena-compaction buffer, `GcState::arena_compact_buf`
(`Vec<u64>`, one word per live arena entry: `object_heap/mod.rs::compact_fields_arena` and
`array_heap.rs::compact_arena` each `try_reserve(live)`). 38,912 B is 4,864 live entries; the
22,528 B request (2,816 entries) shows on the Burn page after the resolution tables changed the
heap's layout. It is fallible and the refusal only skips that collection's compaction, so the
app never sees it — but a skipped compaction leaves the arena fragmented, which is the condition
that made the request fail. Fix shape: compact in bounded slices, or reserve the buffer once
from the pre-reservation budget (`prereserve_config`) while the heap is young. The executor's
resolution caches are no longer a candidate: they are fixed-size tables now (D4 findings).

### G11. Every class an app touches is parsed into RAM, and the sim charges 1.7x the device

Found 2026-09-24 landing `java.time`: with the SDK's port, this app OOM'd in the simulator on
the first page after a sync (`OOM: tried 4096 B — free 12 KB, largest block 3 KB`). The heap
census (`sim.sh -m -l 0` + `heapcensus`, after the first sync, on the Limits page) explained it:

| tree | native heap used | classes parsed | parsed metadata (host) | device estimate |
|---|---|---|---|---|
| main before the round | 353 KB | 61 of 201 | 127 KB | 74 KB |
| with `java.time` in `TimeFormat` | 388 KB | 70 of 226 | 171 KB | 99 KB |
| plus the D4 follow-ups of the same day | 399 KB of 408 | 70 | 171 KB | 99 KB |

Class metadata is parsed lazily on first use and kept for the run (`ClassFile::parsed`,
`OnceCell<Box<Parsed>>`): about 5 KB per class in the simulator's 64-bit model, 3 KB on the
RP2350 (`devB~` in the census line), and it is the single largest consumer of this app's heap —
larger than every live Java object put together (12 KB). A `LocalDateTime.ofInstant(Instant,
ZoneId)` reaches nine classes; the app now formats through `LocalTime`, `ZoneOffset`,
`Duration` and `DateTimeFormatter` only (`util/TimeFormat.java`), which is the four the screens
need, and the sim runs again with about 25 KB to spare.

**Ask:** cheaper parsed metadata (per-method entries are the bulk: name and descriptor slices,
offsets, flags — a packed table would halve them), a census line per class so the cost of an
import is visible, and a sim model that charges device-sized metadata rather than host-sized,
so an app that fits the RP2350 is not refused by the simulator. Until then, an app on this
board should count the SDK classes it touches, not only its own objects.

**Attributed (2026-09-24):** the census now names every parsed class and every part of the
parsed record, and the simulator's allocator can keep the call stack behind every live arena
block (`PICODROID_MEMDIAG_SITES=1`, docs/memory-diagnostics.md). With that, and a mem-diag
firmware on the board beside it, the whole heap has owners. Measured on the same afternoon's
tree, the live bridge, pages turned in order after the first sync:

| page | sim arena used (of 408 KB) | RP2350 heap used (of 408 KB) | RP2350 free / lowest ever |
|---|---|---|---|
| before the first sync | 120 KB | 244 KB | 174 KB |
| Limits | 383 KB | 301 KB | 117 KB / 106 KB |
| Models | 391 KB | 314 KB | 103 KB / 86 KB |
| Burn rate | 396 KB | 317 KB | 101 KB / 78 KB |
| History | 399 KB | 320 KB | 98 KB / 78 KB, largest block 48 KB |

The device is not at the edge: 98 KB free on the worst page, 78 KB at the deepest transient.
The simulator is, by 79 KB, and the ledger says why — pointer width: parsed class metadata
161 KB on the host against 94 KB modelled for the device, the class table 11 against 4.5,
the static field store 14 against 7, the seven dispatch memos 11 against 5. Where the bytes
are on the History page (sim ledger, device model beside it):

| owner | sim | device | what |
|---|---|---|---|
| task stacks, TCBs, queues | 121 KB | 121 KB | JVM 33; `usage-poll` 16.5; `usage-tick` 16.5; 4 pool workers 25; pdb 8.3; fs 8.3; cyw43, flash parker, timer, idle, queues 10.8 |
| parsed class metadata, 73 classes | 161 KB | 94 KB | CP offsets (`usize` each) 43 %, method table 35 %, CP tags 6 %, the `Box<Parsed>` 7 % — the rest is fields, interfaces, bootstrap and exception tables |
| JVM heap storage | 41 KB | 41 KB | slot chunks, the fields arena (14 KB), side tables, arrays, strings, GC buffers; holding 11–26 KB of live Java objects |
| resolution tables | 14 KB | 16 KB | sized on purpose (D4) |
| static field store | 14 KB | 7 KB | a `Vec` of (class name, field name, value), doubled once to 256 entries |
| LittleFS | 12.7 KB | 12.7 KB | read cache, program cache and lookahead each default to the 4 KB block size |
| class table, 226 entries | 10.8 KB | 4.5 KB | |
| dispatch memos, 7 handlers | 10.8 KB | 5.4 KB | main + 4 pool workers + 2 Java threads, 64 rows each |
| JSON pool, frames, misc | 8 KB | 8 KB | |

The device sum (310 KB) is 10 KB under the board's own `nused`: the network stack's sockets
and buffers, which the simulator does not model (host sockets). The LVGL pool is a second
heap with the same disease: the widget tree costs the simulator 1.6× (Burn page 27.0 KB of
its 42.6 KB pool against 16.9 KB of the device's 45.7 KB; `lv=` on the `[memmon]` line).
The whole divergence, term by term, and the plan to close it (M8–M10) are in
docs/parity-audit.md, "2026-09-24 memory-model divergence". Classes that cost the most on
the device: `JSONObject` 6.0 KB, `MainActivity` 5.7, `View` 4.0, `LocalTime` 3.9, `JSONArray`
3.5, `UsageService` 3.5, `Duration` 3.2, `HttpURLConnection` 2.7, `Thread` 2.5,
`LayoutInflater` 2.3, `SharedPreferences` 2.3, `UsageFetcher` 2.2 (its 24 exception-table
entries), `BurnPage` 2.1, `Activity` 2.1.

The levers are tracked as H1–H10 below.

## Heap levers, ranked (H1–H10)

Device bytes on the History page, from the G11 attribution. Each is its own piece of work;
the runtime ones shrink every app and close most of the simulator's gap at the same time
(docs/parity-audit.md, M8).

*App (about 39 KB):*

- **H1. Fold `usage-tick` into the poll thread** — open. `idle()` already waits on the lock;
  wake it every `TICK_MS` and post `tick` from there. −17 KB (a Java thread is a 16 KB stack,
  a TCB and a dispatch memo). `data/UsageService.java`.
- **H2. Read the bridge's reply without `picodroid.json`** — open. `JSONObject`, `JSONArray`
  and the inner class are 9.9 KB of metadata (17.3 KB in the simulator) plus the 2 KB node
  pool, for one flat object whose format the app owns. A `key=value` line format needs a
  short scanner in `UsageFetcher` and a second output mode in the bridge. −12 KB.
- **H3. Format times without `java.time`** — open, only when the budget is wanted. The seven
  classes `TimeFormat` reaches cost 11.9 KB (20.7 KB in the simulator); integer arithmetic on
  the epoch does what the screens need. Undoes part of the 2026-09-24 showcase.

*Runtime, every app (about 36 KB):*

- **H4. `Parsed::cp_offsets` as `Vec<u16>`** — open. No class file is 64 KB (`lnt_offset`
  already rests on it). −17.5 KB here, −52 KB in the simulator: the cheapest large cut, and
  the single biggest term of the sim-versus-device gap. `jvm/src/class_file/`.
- **H5. `MethodInfo` 32 → about 20 B** — open. `code_offset` and `code_len` as `u16`, the
  exception table as an (offset, count) pair into flash instead of a `Vec` per method.
  −12 KB here, −40 KB in the simulator.
- **H6. Static field store keyed by (class index, field index)** — open. 24 → 12 B per
  entry, no doubling `Vec`, no name compare on every `getstatic`. −4 KB, and the same bytes
  on both targets. `jvm/src/static_fields.rs`.
- **H7. Dispatch memos of 16 rows for `JvmChild` and `BgWorker` handlers** — open. Those
  handlers dispatch few natives. −3.5 KB. `native_handler/dispatch_memo.rs`.

*Platform (about 20 KB):*

- **H8. LittleFS `cache_size` 512 B and `lookahead_size` 64 B** — open. The block-size
  defaults cost three 4 KB buffers on every board; preference files are hundreds of bytes.
  −11 KB. `fs/volume.rs::config_for`.
- **H9. `[background_pool] threads = 2` on `pico_display2_w`** — open. The app's only pool
  work is a preference write. −8.5 KB. `board.toml`.
- **H10. The JVM task's 32 KB stack** — open, measure first. The largest single block; it
  needs a high-water reading (`pdb sysmon` task table) before it is touched.

Not levers: the resolution tables (D4 bought them), the JVM heap storage (the live set is a
quarter of it, the rest is pre-reservation and chunking that keeps first-fit placement
stable), the class table.

### Minor

- `--shrink-app` refuses any app that spells a one- or two-letter member name, because the
  release map hands those names to SDK members. The app hit it with `final Palette p` (renamed to
  `palette` in `91c4234c`); five other examples fail the same way. Tracked with the fix in
  `docs/quality-roadmap.md`.

- `board.toml` has no pin-conflict check; collisions between a display, buttons and the CYW43 pins
  are caught only by review.
- `website/.../get-started/build.md` describes `pico_enviro_mon` as 240x135; its `board.toml` says
  240x240.
- `GradientDrawable` honours the colour's alpha as background opacity (so `0x00000000` gives a
  transparent container), which is useful and undocumented.
