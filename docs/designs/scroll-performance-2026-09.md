# Scrolling on the touch board: where the time goes, and what to do about it

> Measured 2026-09-11 against `982c4cf` on `pico_touch_kit` hardware, driving
> `picoclock`'s Set-time screen with `pdb input swipe` and a temporary defmt
> probe in `graphics/lvgl/lifecycle.rs`. The probe timed each band's render and
> flush separately; it was reverted and is not in the tree. **S1 has since
> landed** (§3); nothing else in §3–§7 is started.

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

### S3. Align the refresh period with the tick

`LV_DEF_REFR_PERIOD` is 33 and the tick arrives every 16 ms. `lv_timer` sets
`last_run = lv_tick_get()` with no credit carried (`lv_timer.c:348`), so a 33 ms
period needs three ticks — 48 ms — and loses 15 ms every frame. Setting the
period to 16 costs nothing and recovers the ~19–30 ms of idle measured above.

Mostly subsumed by S2, but worth stating separately because it is a one-line
change that stands on its own if S2 turns out to be awkward.

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

### S6. Find out why a pixel costs 78–138 cycles

The largest single term in the frame is also the least understood. Two
hypotheses were eliminated by measurement (§2) and no replacement was
established, so this should be measured before anything here is optimised.

Three candidates, in the order they are cheap to test:

- **`LV_OBJ_STYLE_CACHE` is 0.** Every style property read walks the object's
  style list and its parent chain for inherited properties, on every draw of
  every object in every band. LVGL v9 added this cache for exactly this. Costs
  a little RAM per object, which §7 would pay for. One flash cycle to A/B.
- **Execution from XIP flash.** The draw code, the style machinery and the font
  glyphs all live in flash behind a small cache. The RP2350 exposes cache hit
  and access counters; reading them across a scroll would settle it directly,
  and is the single most informative measurement left.
- **No assembly blend path.** `LV_USE_DRAW_SW_ASM` is `NONE`. The M33 here has
  DSP instructions but **not** Helium, so `LV_USE_NATIVE_HELIUM_ASM` stays off;
  ARM2D is the only candidate and its benefit on this core is unproven.

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

S6 before S4, S5 or §7. It is one flash cycle, it is the largest term in the
frame, and two confident guesses about it have already been wrong. Spending
weeks on §7 to unlock a style cache that turns out not to matter would be the
expensive version of this mistake.

Then S4 or S5 depending on what S6 says, with §7 scheduled only if S5 is the
answer and the cheap 10-row variant is not.

S7 whenever somebody has the schematic open.

S8 last, and only if idle power or idle CPU becomes a goal. It buys neither
smoothness nor frame rate — S1 already took those — and it is the one item here
whose first step is a bench measurement rather than a code change.

## 9. Open questions

- Does the carrier route the panel's TE pin anywhere reachable? Decides §6
  entirely.
- Is the per-pixel cost dominated by XIP misses, style lookups, or the blend
  inner loop? S6 answers this and gates most of the rest.
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
