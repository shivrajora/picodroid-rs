# Scrolling on the touch board: where the time goes, and what to do about it

> Measured 2026-09-11 against `982c4cf` on `pico_touch_kit` hardware, driving
> `picoclock`'s Set-time screen with `pdb input swipe` and a temporary defmt
> probe in `graphics/lvgl/lifecycle.rs`. The probe timed each band's render and
> flush separately; it was reverted and is not in the tree. **S1 and S3 have
> since landed** (§3, §4), **S6 was measured on 2026-09-11** (§5) — it found
> no hot spot — **S4 landed on 2026-09-12** (§5): a scroll frame on the
> Set-time screen went from 109 ms to 24 ms — and **S5 landed the same day**
> (§5): the flush is asynchronous and the touch board renders into two
> 60-row buffers. What is left is in S4's last subsection (S1, S3, S4, S5, S6
> and S9 now live in [completed/scroll-performance-2026-09.md](../completed/scroll-performance-2026-09.md)) and in §8.

The complaint that started this: scrolling the Set-time screen is slow, choppy,
and tears badly. All three are real, and they are three symptoms of one
arrangement rather than three bugs.

Companion to [psram-rp2350b-2026-09.md](psram-rp2350b-2026-09.md), which §7
here finally gives a reason to build. The ordered implementation plan that
combines both — which of S1-S8 to build, in what order, and where PSRAM fits —
is [psram-lvgl-fluid-scroll-2026-09.md](psram-lvgl-fluid-scroll-2026-09.md).

Completed items: [completed/scroll-performance-2026-09.md](../completed/scroll-performance-2026-09.md) — S1, S3, S4, S5, S6, S9.

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
choppiness, and it is why the fling feels wrong too. S1 (in the completed file) is the fix, and
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

### S8. Let the panel say when to read it

**2026-09-12: landed in the interrupt-accelerated form** (audit F5, WP4).
GP11 is armed for *both* edges and routed to a touch semaphore in the GPIO
ISR; the sampler waits on it with a 10 ms ceiling while a finger is down
and a 50 ms safety net when idle, and scripted touches kick it directly.
That is correct whether the line pulses or holds a level. The bench
measurement below is still owed: with an edge confirmed at touch-down the
idle net can grow to a second, which is the idle-power win.

**2026-09-13: the first run on the touch kit found the driver, not the
wake, wrong.** `Gt911::read_point` answered `None` for a buffer whose
ready bit was clear — "nothing new" read as "nothing there". Two edges per
INT pulse meant the sampler read every report twice, the second time into a
buffer the first had just cleared, so every drag reached LVGL as a stream
of press/release pairs: a ScrollView never left the scroll limit and a
roller took each pair for a tap above or below centre and stepped the other
way. (S1's free-running 10 ms timer could do the same whenever a poll fell
between two reports; the interrupt made it happen on every report.) The
driver now holds the previous report across stale reads and clears the
point only on a report with no fingers (`drivers/gt911.rs`,
`a_stale_buffer_between_two_reports_is_not_a_release`). Both edges stay
armed; the second wake costs one status byte.

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

## 5. Pixels

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

> **Built 2026-09-12**, Stages 1–4, as an opt-in: `hal/rp/psram.rs`, the
> `PSRAM` linker region and `psram_*` MCU keys, the XIP rule in the porting
> guide and in `with_xip_disabled!`, and board.toml `lv_mem_in_psram`. Stage
> 4's measurement says the pool costs 10 % of the frame there (§5 S9), so the
> key is off on `pico_touch_kit`. The bandwidth numbers Stage 1 was to
> produce: 8.7 MB/s writing through the cache, 19.4 MB/s reading uncached,
> 19.7 MB/s reading cached and sequential, at a 75 MHz bus.

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

**S4 is done** (§5), and it changed what the next thing is. A scroll frame is
24 ms of which about 12 ms is pixels; the rest is the tick boundary the frame
lands on, so **S2 is next**: an honest `lv_tick_inc` and a look at what the
main loop does between the render and the next tick. After that, on this
screen, the DatePicker's button matrix (S6b's clip-rejection item) is the
largest per-band cost left. **S5 is done** and its measurement (§5) says what
it was expected to: nothing for a scroll frame, a fifth off a light full
repaint, and nothing for the entry paint, whose cost is render and whose
answer is still S6b.

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
- ~~How much of the 1.6 ms per-band floor is fixed overhead? Decides whether the
  free double-buffering variant in S5 is a win or a wash.~~ **Answered
  2026-09-12: both, by screen** (§5 S5). 1.7 ms of setup per band on the
  clock face against 4 ms of transfer hidden, a win; 7 ms per band on the
  Set-time entry paint, a wash.
- Should the SPI request in `board.toml` be honoured rather than rounded up to
  75 MHz? The panel is running above its rated write clock and nobody chose
  that.
- Does hardware vertical scrolling generalise beyond this one screen, or is it
  a `ScrollView` special case? Worth knowing before S4 is designed, because the
  answer changes where the seam belongs.
- What does GP11 actually do across a gesture on this module — pulse or level,
  which polarity, and is there an edge on release? Decides whether S8 is an
  interrupt-driven sampler or an interrupt-accelerated polled one.

## Amendments

### 2026-09-16 — S2 landed as WP7; the byte swap is gone; what is still open

Checked against `09e7a8b3`. Overrides §8 where they disagree.

- **S2 is built, by another route.** Scheduling-audit WP7 (`d8563ae3`, the
  re-land of `719d43e7`, plus `9d090232`; 2026-09-15) feeds `lv_tick_inc` from
  `tick_source::step_ms()`: one period while the loop keeps up, the tick's
  measured lateness when that is more — deliberately not a raw wall-clock
  delta, because a 15 ms step against the 16 ms refresh period would skip a
  frame per millisecond of jitter (the S3 finding). A source guard pins
  `lv_tick_inc` to its one caller in `graphics/lvgl/lifecycle.rs` and rejects a
  literal step. **Not measured on this board**: the "next 10 ms" §5 S4 expected
  from S2 and from the loop's ordering after `lv_timer_handler` has no number
  yet. Re-run the §1 probe on the Set-time screen before choosing the next item.
- **§2's RGB565 swap is retired, not optimised.** LVGL v9.6.0 (`97635c48`)
  deprecates `LV_COLOR_16_SWAP`, and `09e7a8b3` renders straight into
  `LV_COLOR_FORMAT_RGB565_SWAPPED` (`rgb565-swapped-render-2026-09.md`). The
  ~3 ms/frame swap before `flush_cb` is gone, but RGB565 *images* now pay a
  per-pixel swap while blending, and the change cost +6.5 KB flash. It was not
  timed (that doc §7.4). §2's conclusion — the swap is not where the frame goes —
  stands.
- **LVGL moved under this doc.** Line references into `third_party/lvgl` are
  v9.5 line numbers; v9.6 moves the public headers to `include/lvgl/`.
  `lv_buttonmatrix.c`'s `draw_main` still has no clip rejection in v9.6.0, so
  S6b's third bullet is unchanged.

Open, in the order §8 would now take them:

1. **Measure S2** on `pico_touch_kit` (scroll frame and entry paint).
2. **S6b — the entry paint** (529 ms on the Set-time screen at S4; 205–245 ms in the measurements since the taller bands, `band-height-120-2026-09.md` and `psram-lvgl-fluid-scroll-2026-09.md` §1): the app-side day
   stepper instead of `lv_calendar` is the cheapest; the warm-up profile and
   `draw_main` clip rejection (a vendored-LVGL patch) are the framework ones.
3. **S7 — tearing**: the TE-pin schematic question is unanswered; the ST7796
   init sequence still sends no TEON.
4. **S8 — GP11's behaviour** across a gesture is unmeasured, so
   `touch_sampler.rs` still polls every `IDLE_POLL_MS = 50` idle.
5. **§9's SPI clock question**: `clock_divisors` still floors `scr` to 0 for
   the board's `spi_freq = 62500000`, so the panel runs at 75 MHz.
6. **§7 Stage 5 / the pre-rendered page**: see
   `psram-lvgl-fluid-scroll-2026-09.md`'s amendment.
