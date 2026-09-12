# Scrolling on the touch board: where the time goes, and what to do about it

> Measured 2026-09-11 against `982c4cf` on `pico_touch_kit` hardware, driving
> `picoclock`'s Set-time screen with `pdb input swipe` and a temporary defmt
> probe in `graphics/lvgl/lifecycle.rs`. The probe timed each band's render and
> flush separately; it was reverted and is not in the tree. **S1 and S3 have
> since landed** (§3, §4), and **S6 was measured on 2026-09-11** (§5) — it found
> no hot spot, which makes S4 the remaining lever. Nothing else is started.

The complaint that started this: scrolling the Set-time screen is slow, choppy,
and tears badly. All three are real, and they are three symptoms of one
arrangement rather than three bugs.

Companion to [psram-rp2350b-2026-09.md](psram-rp2350b-2026-09.md), which §7
here finally gives a reason to build. The ordered implementation plan that
combines both — which of S1-S8 to build, in what order, and where PSRAM fits —
is [psram-lvgl-fluid-scroll-2026-09.md](psram-lvgl-fluid-scroll-2026-09.md).

## 1. What was measured

Every scroll step repaints the **entire viewport** — 22 bands, 139,520 px —
regardless of how far the finger moved. `lv_obj_scroll_by_raw` invalidates the
whole scroller (`lv_obj_scroll.c:429`), and nothing downstream narrows it.

A representative steady-state frame, with a 139 ms frame gap:

| Component | Time | Share |
|---|---:|---:|
| CPU render, 22 bands | 81 ms | 67 % |
| SPI shift-out, 22 flushes | 39 ms | 33 % |
| Idle waiting for the next tick | 19 ms | *on top* |
| Post-render bookkeeping | 0.5 ms | — |
| GT911 touch read | 0.6 ms | — |

Across the content on this screen the frame gap runs **126–348 ms, i.e. 3–8
fps**. Per band, rendering a 320x20 area (6,400 px) costs 1.6–10 ms, averaging
3.3–5.9 ms — about **0.5–0.9 µs/px, or 78–138 cycles/px** at 150 MHz. The cost
is diffuse: no single band, widget or draw call dominates.

The SPI half is 38.5–41 ms and stable within 2 ms whatever is on screen, which
is what a fixed pixel count over a fixed-rate bus looks like. Note the bus runs
at **75 MHz, not the 62.5 MHz board.toml asks for**: `clock_divisors(150 MHz,
62.5 MHz)` floors `scr` to 0, giving `pclk / 2`. That is faster, not slower, but
it is not what the file says and it is over the part's rated write clock.

Input is the symptom nobody should have to reason about. The touch read timer
shares `LV_DEF_REFR_PERIOD` with the display refresh, so the panel is polled
once per rendered frame. **A 300 ms swipe produced two position samples, 240 ms
apart, 234 px apart.** The page does not scroll, it teleports. That is the
choppiness, and it is why the fling feels wrong too. S1 below is the fix, and
it has landed.

## 2. What was ruled out

Recorded so nobody re-litigates them. Each was tested, not reasoned about.

**The DatePicker.** `lv_calendar`'s button matrix really does iterate all 56
cells per band with style-descriptor init and text measurement, with no clip
rejection in `lv_buttonmatrix.c`'s `draw_main` — but hiding the widget entirely
moved render from 65–157 ms to 70–131 ms, i.e. nothing.
`setVisibility(INVISIBLE)` maps to `LV_OBJ_FLAG_HIDDEN` (`view_ops.rs:98`), and
LVGL skips hidden objects, so the test was valid.

**The RGB565 byte swap.** `LV_COLOR_16_SWAP` makes `lv_refr.c` run
`lv_draw_sw_rgb565_swap` over every pixel before every flush, which sounds
expensive. It is an unrolled 32-bit loop: ~3 ms per frame.

**Touch polling cost** (0.6 ms) and **post-render bookkeeping** (0.5 ms).

**The 48 ms scheduling quantum** is real but was demoted from the headline to
~15 %. In the simulator it looked dominant — paints 50.7 ms apart, touch
50.8 ms apart, 47 px per step, and an A/B with `LV_DEF_REFR_PERIOD 16` gave
17.0 ms and 23 px — because sim render is ~1 ms, leaving scheduling as the only
visible constraint. **Do not carry sim scheduling conclusions onto this board
without re-measuring.**

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

### S8. Let the panel say when to read it

S1's timer is free-running, so it reads an untouched panel a hundred times a
second forever. At 0.6 ms a read that is about 6 % of a core, and a hundred
wake-ups a second that stop the chip reaching a deeper idle — all of it spent on
the answer "still nobody". This is a power and idle-CPU item, not a smoothness
one: S1 already fixed what the complaint was about.

The line is there. `board.toml` declares `pin_int = 11`, and `hal/rp/touch.rs`
already brings it up as an input at the end of the GT911's reset dance. Nothing
reads it. `hal::gpio::enable_edge_irq` is already in the family-neutral facade,
and the touch board already runs the GPIO interrupt because its two buttons need
it, so the pieces exist.

The sampler's loop would become "block until the edge or a timeout, then read"
in place of "delay 10 ms, then read". Most of the work is in the word *edge*:

- **Not through the button ring.** `hal/event_ring.rs` is drained by the keypad
  indev through `drain_gpio_event`, and `wait_for_button_event` is one global
  semaphore. A touch edge arriving there would have to be filtered back out by
  pin at every drain, in a path that has already had trouble with edges it did
  not expect (the phantom GP15 release). The sampler wants its own semaphore,
  given from the interrupt. Providing that without giving every family a second
  edge-delivery mechanism is the design question, and it is what makes this more
  than an afternoon.
- **The timeout does not go away, and it is the part to get right.** The GT911
  asserts INT when it has a new report; whether it asserts one for the *release*
  depends on the configuration the part boots with, and this driver programs
  none — it uses whatever the module shipped. A pure-interrupt sampler that
  never sees the lift leaves a finger pressed forever. Blocking with a ~50 ms
  timeout and treating the timeout as "read anyway" covers both that and a
  dropped edge, and still cuts the idle rate from a hundred reads a second to
  twenty.
- **The pin is load-bearing at reset.** INT held high across the RST release
  moves the part to bus address 0x14 instead of 0x5D, which `board.toml` says in
  as many words. Whatever arms the interrupt has to stay out of `build_touch`'s
  reset sequence.

Ten minutes on the bench decides the shape before any of it is designed: watch
GP11 across a touch-down, a held finger and a lift, and find out what this module
actually does — pulse per report or level-assert, which polarity, and whether a
release produces an edge at all. The datasheet offers both modes because a
configuration register chooses between them; what matters is the configuration
this panel was shipped with.

## 4. Scheduling

### S2. Make `lv_tick_inc` tell the truth

`lifecycle.rs` calls `lv_tick_inc(16)` with a hard-coded step no matter how long
the frame actually took. LVGL's contract is that this reflects real elapsed
milliseconds. On this board a frame takes 120–200 ms, so LVGL's clock runs at a
fraction of real time and every duration-based thing — the fling animation, the
refresh cadence, the input timer — stretches with it.

Feeding it the measured delta since the previous call is a few lines. The
consequence to watch for: an honest clock means LVGL will see a 120 ms step and
advance animations in one large jump rather than several small ones, so this
wants measuring alongside S1 rather than on its own.

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

### S4. Stop repainting the whole viewport

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

### S5. Overlap the transfer with the render

Today render and DMA strictly serialise. The display is created with one 12.8 KB
band buffer and a NULL second buffer (`lifecycle.rs`), and `draw_buf_flush`
only overlaps when `lv_display_is_double_buffered` — so LVGL renders a band,
blocks for its 1.8 ms of DMA, then renders the next.

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

### S6b. The entry paint is the worst frame on the board, and nobody had measured it

Entering the Set-time screen costs **417 ms**, and the second paint 366 ms,
against 148 ms for a steady scroll frame. That is the visible top-to-bottom wipe
a user reports on entry, and it is three to four times a scroll frame rather than
the same cost.

It is not cache warming. The miss rate on those frames is ordinary (0.58 % and
0.95 %), but the access count is **55.9 M and 31.9 M against 19.5 M** for a
steady frame. The first paints execute roughly three times as many instructions,
then the cost settles. That is layout and first-draw work, not memory.

**S4 does nothing for this.** Hardware vertical scroll makes scroll steps
cheaper and leaves a full repaint exactly where it is.

#### Where the first paint's time goes

A per-band probe — same method, printing each band's draw and flush instead of
only the frame total — localises it. `lv_refr.c:402` runs
`lv_obj_update_layout` inside the refresh timer and before the first band
flushes, so band 1 carries layout as well as its own drawing.

| Band, screen y | Entry paint | 2nd paint | Steady |
|---|---:|---:|---:|
| 1, carries layout | 104 ms | 41 ms | 10-13 ms |
| 2-5, y 64-124 | | 6 ms each | 7-11 ms |
| **6-16, y 144-344** | | **20-24 ms each** | **3-8 ms** |
| 17-18, y 364-384 | | 4 ms each | 2-4 ms |
| 19-22, y 404-464 | | 8-10 ms each | 2 ms each |

Same content and the same scroll offset in all three columns, so the middle rows
are warm-up rather than geometry. Eleven bands at 20-24 ms is about 180 ms, which
made it a bigger item than layout.

#### It is the DatePicker, confirmed by removing it

Those bands are where the calendar sits: a 200 px `DatePicker` at page offset
134, which is a 56-cell button matrix. Hiding it with `setVisibility(INVISIBLE)`
and re-measuring the *entry* paint:

| | Entry paint | 2nd paint | Steady scroll |
|---|---:|---:|---:|
| Calendar visible | 447 ms | 366 ms | 148 ms |
| Calendar hidden | 244 ms | 172 ms | 148 ms |
| **Saving** | **203 ms (45 %)** | **194 ms (53 %)** | **none** |

Band 1 falls from 104 ms to about 10 ms, so the calendar dominated what looked
like layout cost too.

**This reconciles the §2 result that retired the DatePicker.** That test measured
scroll frames, where the widget has already warmed to 3-8 ms per band and really
does cost nothing. Both results are right; they measure different frames. Record
the distinction rather than re-litigating either.

#### What to do about it

In increasing order of scope, and none of them measured yet:

- **Avoid the widget on this screen.** `SetTimeActivity` already has its own year
  and month steppers and uses the calendar only to pick a day of the month. A day
  stepper would remove 200 ms from the first paint outright. Cheapest by far, and
  it is an app change rather than a framework one.
- **Find out what warms up.** The first two paints execute roughly three times
  the instructions of the third with identical content. If that is a computation
  that could happen at construction time, the wipe does not get shorter but it
  stops being the *first* thing a user sees. This wants a per-function profile.
- **Clip rejection in the button matrix.** `lv_buttonmatrix.c`'s `draw_main`
  iterates all 56 cells for every band with no clip test. Note this is probably
  *not* the warm-up mechanism, since steady frames are cheap with the same cell
  count — so treat it as a separate, general improvement, and it means patching
  vendored LVGL.
- **Render out of sight.** The general fix for any expensive first paint, and the
  pre-rendered-page idea in
  [psram-lvgl-fluid-scroll-2026-09.md](psram-lvgl-fluid-scroll-2026-09.md) §5.

### S9. Taller bands — **measured 2026-09-11, the cheapest real win found**

> Implementation plan, written as a hand-off:
> [band-height-120-2026-09.md](band-height-120-2026-09.md). It explains what
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
  of them.

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

## 6. Tearing

### S7. Sync to the panel, or stop needing to

Nothing mitigates tearing today. The ST7796 `INIT_SEQUENCE` never sends TEON
(0x35), board.toml declares no TE pin, and nothing waits on one. Twenty-two
bands land over 120+ ms while the panel self-refreshes near 60 Hz, so two or
three scroll positions are on the glass at once and the seam walks.

Whether a proper fix is even available is a hardware question nobody has
answered: the carrier may not route the panel's TE pin to a header at all. That
is worth ten minutes with the schematic before any of this is planned, because
the answer decides between two very different paths.

If TE is reachable, the fix is conventional: `TEON`, a GPIO interrupt, and
gating band writes on the blanking interval. If it is not, tearing can only be
reduced by making frames cheap enough that fewer seams appear — which makes S4
the tearing fix as well as the speed fix.

## 7. Enabling the PSRAM

[psram-rp2350b-2026-09.md](psram-rp2350b-2026-09.md) designed this on
2026-09-10 and concluded, correctly at the time, that this board did not need
it. That conclusion was reached before anyone tried to scroll a 720 px page on
it. **S5 now gives it a concrete job**: move the 64 KB LVGL pool out of `.bss`
and the double-buffering that RAM currently forbids becomes affordable, along
with the style cache in S6.

The module carries an 8 MB APS6404L, sixteen times the on-chip SRAM, and today
it is wired up nowhere: no linker region, no `psram_kb` key, no boot code. The
only two mentions in the tree are comments saying so.

What has not changed is the advice about *what* to put there. PSRAM is reached
over the same QSPI bus and XIP cache as flash, so the JVM heap and the operand
stacks must stay where they are — the interpreter touches them every opcode.
The candidates remain cold and large: the LVGL pool first, the loaded class
bytes later.

### Scope

**Stage 1 — bring it up and prove it.** Drive the chip-select pad's function
select, issue the APS6404 enter-quad-mode sequence, and program the QMI's second
chip-select timing registers. Must happen in `hal/rp/boot.rs` before the
FreeRTOS arena is handed out. Prove it with a power-on pattern write and read
back across all 8 MB, reported through `pdb sysmon`.

One earlier worry can be retired: the note in `rp2350b.toml` that pins at or
above GP32 need "bank-1 registers ... not written yet" refers to the SIO
drive registers (`gpio_out_set` and friends have no high-word variants in
`hal/rp/gpio.rs`). The chip-select is never driven as a SIO output, and the pad
function select is a per-pin indexed register that already works for any pin the
PAC exposes — the same path `hal/rp/touch.rs` uses for the touch MISO pad. This
should not block Stage 1, but confirm it rather than trust it.

**Stage 2 — make it addressable.** A `PSRAM` region at 0x11000000 in
`mcus/rp/rp2350b.x`, and a `psram_kb` MCU key in
`build_support/flash_layout.rs`. That file models exactly one flash and one RAM
region today, so this is a genuine extension of its geometry rather than a new
parameter — the main cost of the stage.

**Stage 3 — state the XIP rule and audit for it.** This is the stage that will
bite, and the reason to sequence it before any consumer moves. Runtime flash
writes already run with XIP disabled, from RAM, with core 1 parked — the
`with_xip_disabled!` discipline in `hal/rp/flash.rs`. PSRAM lives behind the
same window. **Nothing in PSRAM may be live across a flash write.** Every
existing `with_xip_disabled!` site needs auditing for PSRAM reachability, the
rule needs to join the porting guide beside the flash-write rule, and the
installer in particular must not stage an app image there.

The simulator models neither PSRAM nor the XIP window, so a violation is
hardware-only and silent — the same class of bug as the 32-bit handle dangles,
needing the same kind of on-device test to catch.

**Stage 4 — move the LVGL pool, behind a board.toml opt-in.** Measure the render
throughput cost on the bench *before* it becomes a default anywhere. The open
question from the design doc still stands and matters more now: LVGL allocates
draw buffers from `LV_MEM_SIZE` during rendering, so the pool may be a bad first
tenant precisely because it is not as cold as it looks. If the measurement says
so, the loaded class bytes are the fallback candidate and the 64 KB has to come
from somewhere else.

**Stage 5 — the class bytes.** Only after Stage 4 has a number.

Stages 1–3 buy nothing on their own. That is the honest shape of this work: it
is an enabling change with a three-stage prerequisite, and it should be
scheduled as one piece or not at all.

## 8. Suggested order

S1 is done. Re-judge the complaint against it before starting anything below:
"choppy" may substantially survive or substantially vanish, and that answer
changes how much the rest is worth.

S2 and S3 next, together, for the ~15 %, for animation timing that is simply
correct rather than approximately correct, and because S1's per-sample
timestamps are waiting on them.

S6 is **done**, and it says there is nothing to micro-optimise: 114 cycles per
pixel of diffuse work, a 0.53 % cache miss rate, and no gain from either a style
cache or `-O3` (§5). The guess that spending weeks on §7 might unlock a style
cache that turns out not to matter was the right worry — the style cache does not
matter, and it never needed §7 anyway.

**S4 is therefore the next thing to build.** It is the only item that cuts the
quantity the cost actually scales with. S5 is worth taking alongside it for the
transfer half, starting with the RAM-neutral 10-row variant, and §7 is needed
only if that variant measures badly or the entry paint in S6b becomes the
priority.

S7 whenever somebody has the schematic open.

S8 last, and only if idle power or idle CPU becomes a goal. It buys neither
smoothness nor frame rate — S1 already took those — and it is the one item here
whose first step is a bench measurement rather than a code change.

## 9. Open questions

- Does the carrier route the panel's TE pin anywhere reachable? Decides §6
  entirely.
- ~~Is the per-pixel cost dominated by XIP misses, style lookups, or the blend
  inner loop?~~ **Answered 2026-09-11: none of them** (§5). It is diffuse
  executed work at 114 cycles/px, with the cache hitting 99.5 % of the time.
- What are the extra 36 M instruction fetches in the first paint of a screen
  doing? That is S6b, and it is the largest single frame cost on the board.
- How much of the 1.6 ms per-band floor is fixed overhead? Decides whether the
  free double-buffering variant in S5 is a win or a wash.
- Should the SPI request in `board.toml` be honoured rather than rounded up to
  75 MHz? The panel is running above its rated write clock and nobody chose
  that.
- Does hardware vertical scrolling generalise beyond this one screen, or is it
  a `ScrollView` special case? Worth knowing before S4 is designed, because the
  answer changes where the seam belongs.
- What does GP11 actually do across a gesture on this module — pulse or level,
  which polarity, and is there an edge on release? Decides whether S8 is an
  interrupt-driven sampler or an interrupt-accelerated polled one.
