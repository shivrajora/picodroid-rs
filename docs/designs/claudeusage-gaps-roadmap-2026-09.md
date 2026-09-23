# Platform gaps found building `claudeusage`

**Status: open list; G2 and G3 closed 2026-09-23, nothing else started. D4 added 2026-09-23 as the top priority.**

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

### D4. Every tick of a page swap overruns the slow-handler budget on the RP2350 — **highest priority**

**Status: open (2026-09-23).** Added after the hardware run of the Android-shape round
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
versions: 27 to 35 ms per card is `BigNumber` creating its glyph `ImageView`s on first show,
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

**Repro.** `env $(grep -v '^#' .wifi-creds.env | xargs) PICODROID_NET_TEST_HOST=<PC address>
./scripts/flash.sh --board pico_display2_w --app claudeusage` in the background, wait for
`sync ok`, then `./scripts/pdb.sh --board pico_display2_w input keyevent 20` and watch the RTT log.

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

### D2. `GradientDrawable` LEFT_RIGHT gradients do not render

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

### G1. One font size

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
The app's sprites go with the follow-up commit that moves `BigNumber` onto a `TextView`.

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
hard-code `Palette.CARD`, in one colour.

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

### G8. Keys need a focused widget

There is no `Activity.onKeyDown`; keys reach Java only through `View.setOnKeyListener` on the
focused view. A screen with nothing to focus (a dashboard) must add an invisible focusable `Button`
just to receive keys, as this app and `examples/keydemo` both do. There is also no long-press or
repeat for keys, which would have given four buttons eight actions.

**Ask:** `Activity.onKeyDown` / `onKeyUp` as the fallback when no view consumes the event, and
`KeyEvent.getRepeatCount()` / long-press.

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

### Minor

- `board.toml` has no pin-conflict check; collisions between a display, buttons and the CYW43 pins
  are caught only by review.
- `website/.../get-started/build.md` describes `pico_enviro_mon` as 240x135; its `board.toml` says
  240x240.
- `GradientDrawable` honours the colour's alpha as background opacity (so `0x00000000` gives a
  transparent container), which is useful and undocumented.
