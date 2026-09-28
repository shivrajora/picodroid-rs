# Completed: Scrolling on the touch board: where the time goes, and what to do about it

Items closed out of [scroll-performance-2026-09.md](../designs/scroll-performance-2026-09.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 3. Input

### S1. Sample touch independently of the frame rate — **done**

The single cheapest win, and the one that attacks the reported symptom head-on.
The read costs 0.6 ms and used to happen once per frame, so at 5 fps the finger
was sampled five times a second.

`hal/touch_sampler.rs` now owns the panel on any board whose controller has a
bus of its own. A dedicated task at priority 23 — above the JVM tier, which is
the whole point, since that tier is what spends 120–200 ms rendering — reads it
every 10 ms and pushes each *changed* reading into a 64-entry ring. LVGL's input
callback drains the whole ring in one pass, one queued sample per read, using
`continue_reading`; LVGL processes every position the finger passed through and
renders once at the end. A held finger occupies one slot, not a hundred.

Verified in the simulator with the control channel: a 300 ms scripted swipe
delivers all 13 of its interpolated positions plus the release edge, against the
two the old path saw on hardware.

Two things this does *not* do, deliberately:

- **Per-sample timestamps.** LVGL's fling velocity is a decayed sum over
  `vect_hist`, weighted by `lv_tick_diff` against each sample's timestamp
  (`lv_indev.c:1362`). Handing it honest capture times is only meaningful once
  the tick itself is honest — S2 — because backdating into a clock that advances
  16 ms per 140 ms frame makes timestamps run backwards. Until then every sample
  in a batch carries the same `lv_tick_get()`, which costs the decay and keeps
  the ordering sound.
- **The XPT2046 boards.** That part hangs off the *display's* SPI bus and drops
  its clock for the duration of a read, with nothing serialising it against a
  band flush; only "both happen on the UI task" makes it safe today. They keep
  the inline read, and `touch_private_bus` keeps the ring and the task out of
  their images entirely — the RP2040 has no flash to spend on a feature it
  cannot use.

The `TOUCH_WAS_PRESSED` first-sample discard moved to `sample_panel`, shared by
both paths, and is now gated on `TOUCH_DISCARD_FIRST_SAMPLE`: the XPT2046's RC
network needs it, a capacitive controller reporting finished pixels does not, so
a tap on the touch board registers on the sample that saw it.

What it left behind is S8: the 10 ms timer is a stand-in for the panel's own
interrupt line, which is wired and still unread.

## 4. Scheduling

### S3. Align the refresh period with the tick — **done**

`LV_DEF_REFR_PERIOD` was 33 and the tick arrives every 16 ms. `lv_timer` sets
`last_run = lv_tick_get()` with no credit carried (`lv_timer.c:348`), so a 33 ms
period needed three ticks and spent two of them waiting. The period is now 16,
which costs nothing and recovers the ~19–30 ms of idle measured above.

Measured in the simulator before and after with a temporary probe that forced a
full-screen invalidation on every tick — so the refresh period was the only
thing limiting the paint rate — and reported how many ticks each paint had
waited for. The probe was reverted and is not in the tree.

| `LV_DEF_REFR_PERIOD` | Ticks waited per paint | Paint interval |
|---|---:|---:|
| 33 | 3, on every one of 1,105 paints | 50 ms |
| 16 | 1, on every one of 4,800 paints | 16 ms |

There is no distribution to report: the quantisation is exact, which is what
makes this a mismatch rather than a tuning choice.

What it does and does not buy. Nothing about the *work* per frame changed, so on
the touch board the 120 ms of render and transfer is untouched and the gain
today is the 19-30 ms of idle measured in §1. What it really buys is that S4
will not be immediately capped: with a 48 ms quantum in place, a frame whose
work fell to 13 ms would still have landed at 20 fps. The ceiling is now the
tick itself, 62 fps.

Not yet re-measured on hardware — the before/after above is the simulator,
where the quantum is the same because the tick and the period are both
family-neutral. The 19-30 ms it should recover on the board is the figure from
§1, not a new measurement.

Because the tick and the period have to agree, a guard test in
`executors/tick_source.rs` reads the define out of `lvgl/lv_conf.h` and asserts
it equals `TICK_PERIOD_MS`. There were already three hard-coded 16s across two
files; this was the fourth, and the only one that disagreed.

Mostly subsumed by S2, but it stood on its own as a one-line change, which is
why it went first.

## 5. Pixels

### S4. Stop repainting the whole viewport — **done 2026-09-12**

> Built as `graphics/lvgl/hw_scroll.rs` (state and events), `hw_scroll_math.rs`
> (the row arithmetic, host-tested), `lvgl/hw_vscroll.c` (the two questions
> that need LVGL's private headers), `VSCRDEF`/`VSCRSADD` in both panel
> drivers behind two defaulted `HalDisplay` methods, and an emulation of the
> panel's scroll registers in the simulator's display. `board_cfg::hw_vscroll`
> turns it on for a portrait ST7796 (`driver` + `madctl`); `hw_vscroll = false`
> in `[display]` is the A/B switch. Measured below, §"What it bought".

The structural fix, the biggest win, and the most work.

The ST7796 can scroll its own frame memory: `VSCRDEF` (0x33) defines a top fixed
area, a scrolling area and a bottom fixed area, and `VSCRSADD` (0x37) moves the
start line within it. This screen maps onto that exactly — the 44 px header is
the top fixed area and the 436 px scroller is the scroll area. Scrolling then
costs only the newly exposed rows: **about 15,000 px for a 47 px step instead of
139,520, a 9x reduction** in both render and transfer, and far fewer seams for
§6 to worry about.

The work is not in the panel driver, which is trivial, but in teaching the
framework that a scroll can be a hardware operation. LVGL has no notion of it:
it will keep invalidating the whole scroller and rendering every band. Making
this real means intercepting the scroll at the `ScrollView` seam, tracking the
panel's scroll origin, translating LVGL's coordinates for the rows that do need
drawing, and handling the wrap at the end of the scroll area. It is a framework
feature with a design of its own, and it only pays on a full-width vertical
scroll — any other invalidation still costs full price.

Prerequisite worth noting: the screen would have to stop being one 720 px page
inside a scroller if partial invalidation is ever to help elsewhere.

#### How it was built

LVGL is not patched. `lv_obj_scroll_by_raw` moves the children, sends
`LV_EVENT_SCROLL`, and only then invalidates the whole scroller; the display
sends `LV_EVENT_INVALIDATE_AREA` with a mutable area before it records one.
Those two hooks are the whole seam:

- **Per step**, the `SCROLL` handler asks `hw_vscroll.c` whether this scroller
  may be moved by the panel: full width after clipping by its ancestors, on the
  active screen, not transformed, and nothing inside its rows that would move
  wrongly. The rule the panel imposes is that every pixel in the band that does
  not move with the children must look the same on every row — a flat fill, a
  horizontal gradient, a side border pass; a top or bottom border line, a
  rounded corner, an image, a vertical gradient refuse. Static things drawn
  *over* the band (a floating child, a later sibling, anything on the top or
  system layer) come back as overlays to repaint after the step, up to four and
  half the band's rows, past which a repaint is cheaper.
- When it may: the origin advances by the step, every area already queued for
  redraw inside the band is stretched by the same distance (its pixels moved
  too — `lv_display_t::inv_areas`, hence the C), the overlays and the scrollbar
  thumb are queued, and one narrowing is armed: the scroller's own invalidation
  that follows the event is rewritten to just the rows that scrolled in.
- **At `LV_EVENT_RENDER_START`** the panel is told (`VSCRDEF` once per band,
  `VSCRSADD` per change), right before the first band of the refresh that draws
  the strip, so the shift and the fill of the stale rows land within the same
  few milliseconds.
- **Every flush** translates display rows to memory lines through the current
  rotation, splitting a band at the wrap. So a repaint of anything — the header,
  a pressed button, a dialog, an entire new screen — lands where the panel shows
  it, and the rotation never has to be undone. It returns to the identity for
  free whenever the whole screen is invalidated, because that repaint overwrites
  every line anyway.
- **A step the panel cannot take** — sideways motion, a second scroller with a
  different band while the panel still holds a rotation, a scroller that
  refuses — falls back to what LVGL was going to do. The picture is the same
  either way; only the cost differs.

Two things the first bench run found that were not in the plan, both fixed on
the `ScrollView` itself and both good for every board:

- **The theme's scrollbar transition made the drag repaint in full.** The
  default theme binds a second scrollbar style to `LV_STATE_SCROLLED` (thumb
  opacity 40% to 100%) with an 80 ms transition, and each animation tick — and
  the state change itself, twice per gesture — invalidates the whole scroller.
  With the tick still lying (S2), 80 ms of transition was five full 80 ms
  frames: the entire finger-down phase. `ScrollView` now removes that binding
  and the transition, so a scroll changes no style at all.
- **A translucent, round-ended thumb has to be repainted end to end** after a
  shift, and a thin 264-row clip through the calendar cost 15-18 ms — more than
  the strip. `ScrollView`'s thumb is now opaque, and for a flat opaque thumb only
  its two ends are repainted (about 35 rows each): wherever the old thumb's
  moved pixels and the new thumb overlap, the pixels are already that colour.

The one visible change: `ScrollView` draws no border and has square corners,
as Android's does — the theme's card outline would have refused every one.
Verified in the simulator, which emulates the panel's scroll registers: nine
screenshots along a scripted tap-swipe-fling-drag-tap sequence are
pixel-identical with the feature on and off, across 255 hardware steps.

#### What it bought

Same firmware, same scripted gesture (the §6 recipe of
[band-height-120-2026-09.md](../designs/band-height-120-2026-09.md): tap Set time, three
400 ms swipes up, three back down), same probe, one build flashed twice with
`hw_vscroll = false` and without. Frames while the finger or the fling is
moving the page, clock-face repaints and the entry paint excluded:

| | Frames | Rows rendered | Render | Flush | Frame | fps |
|---|---:|---:|---:|---:|---:|---:|
| Panel scroll off | 193 | 399 | 64.9 ms | 33.0 ms | 100.9 ms | 9.9 |
| of which full 436-row repaints | 174 | 436 | 71.5 ms | 36.1 ms | 109.3 ms | 9.1 |
| **Panel scroll on** | 203 | **36** | **10.8 ms** | **2.0 ms** | **23.8 ms** | **41.9** |
| of which full 436-row repaints | **0** | | | | | |

Eleven times fewer rows per frame, render down 6x, transfer down 16x, and
the frame rate up 4.2x. The 23.8 ms is not pixels any more — see below.

**The first bench run looked different**, and the difference is the two
`ScrollView` fixes above. Before them the finger-down phase was still full
repaints (7 x 80 ms per swipe: the scrollbar's opacity transition) and each
fling step was 21-24 ms, of which 15-18 ms was the thumb: 2 bands, ~250 rows.
After them a step is 3 bands — the strip and the thumb's two ends — and
36 rows on average.

#### What is left in a scroll frame now

Per-band probe lines on the same gesture, on this build:

- **The thumb ends cost 1.9 ms each**, 369 of them averaged: the per-band
  setup and nothing else. The strip is the whole variable cost.
- **A strip crossing the DatePicker costs 16-18 ms whatever its height.** The
  bottom rows of the band are cheap (1-6 ms for the full-width rows the finger
  exposes scrolling up); a *one-row* strip at the top of the band, exposed
  when content moves down or the fling bounces back, measured 16.3-18.4 ms
  whenever page rows 134-334 sat under it. That is S6b's third bullet — the
  button matrix's `draw_main` walks all 56 cells for any band that touches it,
  clip or no clip — and it is now the largest single term in a scroll frame on
  this screen. It needs the vendored LVGL patched, or the screen to stop using
  the widget, and either is out of scope here.
- **Frames land on tick boundaries.** Render plus flush averages 10 ms for the
  frames that took one 16 ms tick and 15 ms for those that took two; the rest
  of a tick is the main loop's Java dispatch after `lv_timer_handler`. Half the
  frames took two ticks. S2 (an honest `lv_tick_inc`) and the loop's ordering
  are where the next 10 ms is, not in pixels.
- **The entry paint is untouched**, as §5 S6b predicted: 529 ms on this run.

### S5. Overlap the transfer with the render — **done 2026-09-12**

> Built as two defaulted `HalDisplay` methods, `write_pixels_start` and
> `write_pixels_wait`, a `SpiAsyncWrite` extension trait the RP SPI layer
> implements by splitting its DMA write into a start that keeps the bus lock
> and a finish that collects the completion semaphore, and a `pending` flag
> in both panel drivers so that every command — `set_window`, `VSCRSADD`, a
> sleep — collects the band in flight before it deselects the panel.
> `lifecycle.rs` sets LVGL's `flush_wait_cb` and never calls
> `lv_display_flush_ready`; `flush_cb` starts the transfer and returns. The
> second buffer is board.toml `draw_buffers = 2` (`build_support` refuses it
> on a board whose touch controller shares the display's SPI bus), and
> `pico_touch_kit` ships `band_height = 60, draw_buffers = 2`: the same
> 76,800 B as the single 120-row buffer S9 landed. Measured below.

Before this, render and DMA strictly serialised. The display was created with
one band buffer and a NULL second buffer (`lifecycle.rs`), and `draw_buf_flush`
only overlaps when `lv_display_is_double_buffered` — so LVGL rendered a band,
blocked for its DMA, then rendered the next.

Two changes are needed together, and neither works alone:

1. **A second buffer**, so LVGL has somewhere to render while the first drains.
2. **An asynchronous flush.** `flush_cb` currently blocks on the DMA semaphore
   and then calls `lv_display_flush_ready`. It would instead start the transfer
   and return, with the existing DMA completion interrupt signalling ready.

Done properly this hides most of the 39 ms, since a band's render (3.3–5.9 ms)
comfortably exceeds its DMA (1.8 ms).

The catch is RAM: 520,304 / 532,480 bytes are used, **97.7 %**, leaving ~12 KB
against an 8 KB main-stack floor. A second 12.8 KB buffer does not fit. Two
options:

- **Halve the band height to 10 rows and keep two buffers of 6.4 KB.** Same
  total memory, double-buffered for free. The risk is that per-band fixed
  overhead doubles with 44 bands instead of 22, and how much of the measured
  1.6 ms floor is fixed versus per-pixel is exactly what S6 answers. Cheap to
  test, so test it rather than argue about it.
- **Free the memory elsewhere** — which is §7.

#### What it bought

S9 had since made the single buffer 120 rows and ruled out halving a 20-row
band (§5 S9); the variant built is the one it allowed, two 60-row buffers on
top of the taller band. Measured with a temporary per-frame probe in
`lifecycle.rs` (render = the gaps between flush calls, wait = the time LVGL
spent in `flush_wait_cb`, both reverted), the same firmware flashed twice,
radio configured either way, three workloads: the §6 scroll gesture of
[band-height-120-2026-09.md](../designs/band-height-120-2026-09.md), then five rounds of
BACK to the clock face and a tap back into Set time — ten full 480-row
repaints, alternating a light screen and the heaviest one on the board.

| Frame | 1 x 120 rows (4 bands) | 2 x 60 rows (8 bands) | Change |
|---|---:|---:|---:|
| Clock-face repaint, 480 rows | 63.9 ms = 34.1 render + 29.1 wait | **51.3 ms** = 40.7 render + 9.5 wait | **-20 %** |
| Set-time entry paint, 480 rows | 230.4 ms = 200.7 render + 29.1 wait | 229.4 ms = 227.8 render + 0.5 wait | 0 % |
| Set-time scroll frame (S4), ~40 rows | 12.4 ms, 0.3 wait | 12.1 ms, 0.2 wait | 0 % |

Three different answers from one mechanism, and the S9 cost model predicts
all three. The transfer of a 60-row band is 4 ms; hiding it pays off only
where the extra band's setup costs less than that. On the clock face a band
costs 1.7 ms of setup, so the four extra bands cost 7 ms and hide 20 ms of
transfer. On the Set-time entry paint a band costs 7 ms — the DatePicker's
button matrix walks all 56 cells for every band that touches it, §5 S6b —
so the four extra bands cost 27 ms and hide 29: a wash to the millisecond.
A hardware-scrolled frame moves ~25 KB and had 0.3 ms of transfer to hide.

So the double buffer is never a loss and sometimes a fifth, for no memory.
What it is *not* is a way past the entry paint: that frame is render, and the
transfer it serialised was 13 % of it. The residual 9.5 ms of wait on the
clock face is the bands that render faster than they drain; a third buffer
would take some of it, and a 120-row pair would take all of it and the
per-band setup too, but the first costs 38 KB and the second 77, and the
arena has neither (§5 S9). The measured wait is the number to re-read if a
board ever does.

The asynchronous flush is on for every board, single-buffered or not — with
one buffer LVGL waits before it renders again, which is where it used to
block, so nothing changes there except that a frame's last band drains while
the UI task runs Java. `draw_buffers = 2` is what a board opts into, and
only a board whose panel has the SPI bus to itself may: the XPT2046 reads the
touch panel over the display's bus from the UI task, and a band still
streaming under it would be corrupted. `build_support` refuses the pair.

### S6. Why a pixel costs 114 cycles — **measured 2026-09-11, three more hypotheses dead**

The largest single term in the frame was also the least understood. It has now
been measured on hardware, and the answer is that there is no hot spot to fix.

Method: a temporary probe in `lifecycle.rs` split each frame into CPU render and
SPI flush, accumulated across the frame's bands and reported once per painted
frame, with the RP2350's `XIP_CTRL` hit and access counters cleared at frame
start and read at frame end. Input was scripted through `pdb input` — a tap on
Set time, then three 400 ms upward swipes — so every variant below saw an
identical gesture. The probe was reverted and is not in the tree.

**Steady-state scrolling, 49 frames:**

| Quantity | Value |
|---|---:|
| CPU render | 107.2 ms |
| SPI flush | 38.7 ms |
| Frame | 147.7 ms (6.8 fps) |
| Per pixel, over 140,800 px | 114 cycles |
| XIP cache accesses | 19.5 M per frame |
| XIP cache misses | 103 k per frame |
| **XIP miss rate** | **0.53 %** |
| XIP accesses per cycle | 1.22 |

**The renderer is not starved for instructions.** The cache serves more than one
access per cycle at a 0.53 % miss rate, so execution from flash is not the
constraint. That retires the measurement this section used to call the most
informative one left.

**Two A/B tests, same gesture, both negative:**

| Variant | Steady render | Cost | Verdict |
|---|---:|---:|---|
| Baseline, C at `-Os` | 108.9 ms | — | — |
| `LV_OBJ_STYLE_CACHE 1` | 113.7 ms | +480 B flash | no effect, reverted |
| MCU `c_opt_level = "3"` | 115.1 ms | +92 KB flash | no effect, reverted |

Both came out marginally *worse* than baseline, within run-to-run noise. The
style cache is worth a note of its own: it costs 8 bytes per widget out of the
LVGL pool rather than `.bss`, so it was affordable all along and never needed
§7's freed RAM — it simply does not help. Keep it off.

`-O3` failing is the more interesting one. Better codegen not helping, and
inflating the image slightly hurting, would fit a fetch-stalled loop — except
the counters say the fetches are hitting. What both results together say is that
the work is real, executed, and diffuse: not style-list walks, not instruction
supply, not the quality of the generated code.

**So five hypotheses are now dead**: the DatePicker and the RGB565 byte swap
(§2), style lookups, C optimisation level, and XIP instruction fetch. 114 cycles
per pixel is simply what this renderer costs for this widget tree.

**The conclusion that matters: stop trying to make a pixel cheaper and render
far fewer pixels.** Because the cost is diffuse and scales with area, S4's 9x
reduction in pixels per scroll step should convert almost directly into a 9x
reduction in render time. S4 no longer needs S6's permission — S6 has given it.

One candidate is left untested, and is not recommended first:
`LV_USE_DRAW_SW_ASM` is `NONE`. This M33 has DSP instructions but **not**
Helium, so `LV_USE_NATIVE_HELIUM_ASM` stays off; ARM2D is the only option and
its benefit on this core is unproven. Given that `-O3` on the same loops bought
nothing, expect little.

### S9. Taller bands — **measured 2026-09-11, the cheapest real win found**

> Implementation plan, written as a hand-off:
> [band-height-120-2026-09.md](../designs/band-height-120-2026-09.md). It explains what
> a "band" is, carries the bench recipe, and states the heap trade.

Every band holds exactly the same number of pixels, 320 x 20 = 6,400. Yet within
one steady frame bands cost between 1.9 ms and 11 ms, and on a first paint up to
24 ms. A six-fold spread at constant area means the cost is not per-pixel at all:
it tracks how many widgets intersect the band, and a widget spanning eleven bands
has its setup done eleven times.

So the lever is **fewer bands**, which means a bigger draw buffer. The only place
on this board with that much SRAM is the FreeRTOS arena, so the test traded some
of it: MCU `heap_kb` 408 -> 344 (frees 65,536 B) and board `band_height`
20 -> 120 (costs 64,000 B), taking a full repaint from 22 bands to 4. Total RAM
went *down* slightly and main-stack headroom went *up*, to 13,696 B.

Identical scripted gesture, same probe as S6:

| | Render | Flush | Frame | fps |
|---|---:|---:|---:|---:|
| `band_height = 20`, 22 bands | 108.9 ms | 39.0 ms | 149.6 ms | 6.69 |
| `band_height = 120`, 4 bands | **59.2 ms** | 35.9 ms | **96.3 ms** | **10.39** |
| Change | **-46 %** | -8 % | **-36 %** | **+55 %** |

The entry paint improves by as much: 447 ms -> 248 ms, and the second paint
366 ms -> 174 ms. The flush gain is a small bonus from four window-setup command
sequences instead of twenty-two.

**A cost model, from two points.** 18 fewer bands saved 49.7 ms, so a band costs
about 2.76 ms of pure setup on this screen. That extrapolates to ~54 ms of render
at one band, which is the floor this widget tree can reach — around 54 cycles per
pixel of genuinely per-pixel work.

**A third point, measured with WiFi associated**, confirms the model and is the
configuration to ship: `band_height = 60` (8 bands), `heap_kb = 384`.

| Config | Bands | Render | Flush | Frame | fps | Arena cut | Entry paint |
|---|---:|---:|---:|---:|---:|---:|---:|
| `band_height 20` | 22 | 108.9 ms | 39.0 ms | 149.6 ms | 6.69 | — | 447 ms |
| `band_height 60` | 8 | 72.8 ms | 36.5 ms | 110.6 ms | 9.04 | 24 KB | 293 ms |
| `band_height 120` | 4 | 59.2 ms | 35.9 ms | 96.3 ms | 10.39 | 64 KB | 248 ms |

The model predicted 70.2 ms render and 9.3 fps for the 8-band case against 72.8
and 9.04 measured, so per-band setup really is close to linear here.

**The arena cost is smaller than feared, and WiFi barely moves it.** `pdb sysmon`
low-water marks, after driving the same gesture:

| Arena | Network | Lowest free | Peak use |
|---|---|---:|---:|
| 352,256 B (`heap_kb 344`) | down | 76,712 B | 275,544 B |
| 393,216 B (`heap_kb 384`) | **up, IP assigned** | 116,896 B | 276,320 B |

Peak arena use differs by under 800 bytes between the two, so **associating with
an access point costs essentially no arena** — the network stack's buffers are not
coming from it. The worry that a W board could not afford this trade was
unfounded, and the 8-band config keeps 114 KB of margin with the radio up.

Consequences:

- **Most of the win is already captured at 4 bands.** Going to 2 bands would buy
  perhaps 6 ms more for another 77 KB of arena. Not worth it.
- **S5's RAM-neutral variant is refuted.** Halving band height to 10 rows for two
  buffers would take a full repaint to 44 bands and add roughly 60 ms of setup —
  far more than the ~36 ms of transfer it could hide. Do not build it. If
  double-buffering is wanted, it has to be on top of *taller* bands, not instead
  of them. (It was, as 2 x 60 rows — §5 S5.)

**This also re-prices §7 entirely.** The LVGL pool is 65,536 B and a 120-row band
buffer needs 64,000 B more than a 20-row one. Moving the pool to PSRAM funds the
taller buffer almost exactly, with 1,536 B left over, and the JVM arena never has
to shrink. That turns §7 from an enabling change with a speculative payoff into
one with a measured one: **+55 % frame rate**. It is now the strongest argument
for building PSRAM support, stronger than the double-buffering case that
originally motivated it.

Not verified by eye. The app runs and flushes four bands per paint, but nobody has
confirmed the picture is correct at this band height, and tearing should if
anything improve with fewer seams.

**Landed 2026-09-12** as `band_height = 120`, `heap_kb = 344`, re-measured on
the landed build at 61.6 ms render / 98.3 ms frame / 10.2 fps with the radio
up. The §7 re-pricing above was then tested rather than trusted: with the pool
in PSRAM the same gesture renders in 71.7 ms and the frame is 109.5 ms
(9.1 fps), so the pool stays in `.bss` and the arena cut stands — the
draw-task churn is the part of the pool that is hot. Details in
[band-height-120-2026-09.md](../designs/band-height-120-2026-09.md) §9.
