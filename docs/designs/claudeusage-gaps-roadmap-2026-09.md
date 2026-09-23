# Platform gaps found building `claudeusage`

**Status: open list, nothing started (2026-09-21).**

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

### G2. No arc or ring gauge

Ring gauges are the signature look of the ESP32 usage monitors, and LVGL has `lv_arc`. There
is no SDK widget for it. The app uses bars throughout.

**Ask:** an arc widget (progress, range, colour, stroke width, start and sweep angles).

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
