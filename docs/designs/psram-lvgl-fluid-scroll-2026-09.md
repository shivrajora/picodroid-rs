# Fluid scrolling on the touch board: the PSRAM plan, and what LVGL buys with the room

**Status: planned 2026-09-11, not started.** The implementation plan for the
PSRAM feasibility study in [psram-rp2350b-2026-09.md](psram-rp2350b-2026-09.md)
and the profiling in
[scroll-performance-2026-09.md](scroll-performance-2026-09.md). Those two say
*whether* and *why*; this one says *in what order, and what each step is worth*.
Every code fact was re-verified against the tree on 2026-09-11.

The goal is fluid scrolling on `pico_touch_kit`, which today runs at 3-8 fps.

## 1. The honest shape of this

**PSRAM makes nothing faster.** Two thirds of the frame is CPU render out of
SRAM, and PSRAM is slower memory reached over the same QSPI bus and XIP cache
as flash. Putting hot data there makes the frame worse, not better.

What PSRAM buys is a *budget*. SRAM is 97.7 % full and that is the reason
several LVGL optimisations are currently unaffordable. Today:

| Consumer | Bytes |
|---|---:|
| FreeRTOS arena (the JVM heap, `heap_kb = 408`) | 417,792 |
| LVGL pool (`LV_MEM_SIZE`, a `.bss` array) | 65,536 |
| Band buffer (320 x 20 x 2) | 12,800 |
| Everything else in `.data` + `.bss` | ~24,136 |
| **Free, against an 8 KB main-stack floor** | **12,216** |
| Total SRAM | 532,480 |

Moving the LVGL pool out of `.bss` returns 64 KB. That is the whole point of
this work, and everything in §4 spends it.

## 2. Fluid scrolling does not need PSRAM

Worth stating before planning three stages that buy nothing on their own. The
two largest wins in the profiling need no memory at all:

- **S4, hardware vertical scroll** (`VSCRDEF`/`VSCRSADD` on the ST7796) cuts a
  scroll step from 139,520 px to about 15,000. Zero RAM. Neither command
  appears in `drivers/st7796.rs` today.
- **S2 and S3**, an honest `lv_tick_inc` and a 16 ms refresh period, recover
  roughly 15 % of the frame. Zero RAM.
- **S5's cheap variant**, two 6.4 KB buffers at a 10-row band height instead of
  one 12.8 KB buffer at 20 rows, is RAM-neutral.

So PSRAM is not on the critical path. It is what makes the *good* versions of
S5 and S6 affordable, and it is the only thing that makes §5 possible at all.
Sequence it accordingly: §7 puts the free wins and the one decisive measurement
ahead of every stage below.

## 3. Bringing PSRAM up

Three stages, in this order, none of which produces a user-visible change.

### Stage 1 — boot it and prove it

The bootrom leaves the QMI's second chip select alone. Bringing PSRAM up means,
in `platforms/rp/src/hal/rp/boot.rs`, before the FreeRTOS arena is handed out:

1. Set GP47's pad function to `XIP_CS1`.
2. Issue the APS6404L's enter-quad-mode sequence through the QMI's direct mode.
3. Program the M1 read, write and timing registers for the part.

Three details decide whether this works:

- **The 8 µs `tCEM` limit is the one to get right.** The APS6404 is
  pseudo-static: it self-refreshes internally, but only while deselected, so
  chip select may not stay asserted indefinitely. The QMI's `MAX_SELECT` /
  `MIN_DESELECT` timing fields are what break a long burst into refresh-safe
  chunks. A DMA read of a 12.8 KB band is exactly such a burst, so getting this
  wrong produces corruption that only appears under load.
- **The clock divisor rounds the way the display's did.** The part is rated
  133 MHz and the system clock is 150 MHz, so the divisor lands on 2 and the bus
  runs at 75 MHz. That is inside the rating, unlike the display SPI, which
  `clock_divisors` silently rounded *above* its rated write clock. Record the
  number this time rather than inheriting it.
- **The bank-1 worry is retired, but confirm it.** `rp2350b.toml` warns that
  pins at or above GP32 need bank-1 registers that `hal/rp/gpio.rs` does not
  have. That warning is about the SIO drive registers, and GP47 is never driven
  as a SIO output — the pad function select is a per-pin indexed register that
  already works for any pin the PAC exposes, the same path `hal/rp/touch.rs`
  uses for the touch MISO pad. Confirm rather than trust, and add a debug
  assert in `gpio.rs` so a future `1u32 << pin` on a bank-1 pin fails loudly
  instead of silently.

**Proving it needs care.** A pattern test small enough to sit in the XIP cache
proves only that the cache works. The readback has to go through the
non-cached XIP alias, or sweep far enough past the cache to make it irrelevant.
Do both: a full 8 MB write-and-verify sweep at boot behind a feature, and a
cache-bypassing spot check that stays in the default build. Report through
`pdb sysmon`, and while the harness is there, **measure PSRAM read and write
bandwidth** — §5 cannot be judged without that number.

### Stage 2 — make it addressable

A `PSRAM` region at `0x11000000` in `platforms/rp/mcus/rp/rp2350b.x`, and a
`psram_kb` key rendered by `crates/build_support/flash_layout.rs`.

That file models exactly one flash and one RAM region, so this is a genuine
extension of its geometry. Concretely:

- `FlashLayout` gains `psram_origin` and `psram_len`, and the struct name
  becomes a slight lie. Leave the name — it is the shared geometry type every
  board includes, and renaming it touches far more than this work.
- `render_memory_x` emits the region **only when `psram_kb` is present**. The
  tests at the bottom of the file pin the rendered output byte-for-byte for
  boards that have none, and those assertions should not move.
- `rust_consts` gains `PSRAM_ORIGIN` / `PSRAM_LEN` so the firmware and the
  `LV_MEM_ADR` define in §4 read the same number the linker does.
- Put `psram_kb` in the **MCU** toml beside `flash_kb`. Both are properties of
  the module rather than the die, which is already an acknowledged wart in
  `rp2350b.toml` — a descriptor exists for that variant precisely because
  `flash_kb` is MCU-only. Consistency beats a second mechanism here. The
  *policy* keys (which tenant goes to PSRAM) belong in board.toml.

Note that the `PSRAM` region gets no `.psram` output section at first. Nothing
is linked there in Stage 2; §4's first tenant takes its address as a constant,
not as a section. A second tenant is what forces a real sub-allocator.

### Stage 3 — state the XIP rule, and audit for it

The stage that will bite, and the reason to do it before any tenant moves.
PSRAM lives behind the same XIP window that runtime flash writes switch off.

**Good news first.** The hazard is narrower than the feasibility doc implies,
in two ways worth writing down:

- **PSRAM contents survive a flash write.** The part self-refreshes while
  deselected. What fails is *access* during the window, not retention.
- **The audit surface is two call sites**, both in
  `platforms/rp/src/hal/rp/flash.rs`: the `with_xip_disabled!` wrappers around
  `flash_range_erase` and `flash_range_program`. Both already run from RAM with
  interrupts masked on the calling core and core 1 parked
  (`hal/rp/core1_park.rs`). Nothing reachable from either closure touches
  PSRAM today.

**The rule, for the porting guide, beside the existing flash-write rule:**
between XIP-off and XIP-restore, no code on either core may read or write
PSRAM — and that includes core 1's park loop. Two corollaries:

- The installer must not stage an app image in PSRAM.
- `with_xip_disabled!` must restore the M1 window as well as M0. This is the
  second window that the existing "restore fast XIP after flash ops" rule now
  has to cover, and the sim models neither.

A violation is hardware-only and silent, the same class of bug as the 32-bit
handle dangles. The on-device test that catches it: install a package (which
forces real flash erases and programs) while the UI is scrolling, with the
LVGL pool already in PSRAM. That belongs in `hil-tests.conf` as part of Stage 4,
not as an afterthought.

## 4. What LVGL does with the 64 KB

### 4.1 Moving the pool is one define

`LV_USE_STDLIB_MALLOC` is `LV_STDLIB_BUILTIN`, and
`third_party/lvgl/src/stdlib/builtin/lv_mem_core_builtin.c` already has the
hook: with `LV_MEM_ADR` nonzero, `lv_mem_init` calls
`lv_tlsf_create_with_pool((void *)LV_MEM_ADR, LV_MEM_SIZE)` instead of
declaring the static array. So the move is `build_support/lvgl.rs` emitting
`LV_MEM_ADR` from the Stage 2 constant when the board opts in, next to where it
already emits `LV_MEM_SIZE` from `lv_mem_kb`. Board key: `lv_mem_in_psram`.

Cheap to try and cheap to revert, which is what a first tenant should be.

### 4.2 The open question has a fix, not just a measurement

Both prior docs flag the same risk: LVGL allocates draw buffers from
`LV_MEM_SIZE` during rendering, so the pool may be a bad first tenant precisely
because it is not as cold as it looks. That is real. The chain is
`lv_draw.c:503` -> `lv_draw_buf_create` -> `buf_malloc` -> `lv_malloc`, so with
the pool in PSRAM every blended or transformed layer becomes a per-pixel write
target on the QSPI bus.

It is fixable rather than merely measurable. `lv_draw_buf_get_handlers()`
returns a mutable pointer to the default handler struct, so after `lv_init()`
we install our own `buf_malloc_cb` / `buf_free_cb` that serve render targets
from a small SRAM arena and leave everything else in PSRAM. That is the
arrangement we wanted anyway: **cold metadata in PSRAM, hot pixels in SRAM.**

Sizing it: `LV_DRAW_LAYER_SIMPLE_BUF_SIZE` is 8 KB, so a 16 KB SRAM arena
covers the simple-layer case with room for two. A request that does not fit
falls back to the pool and logs once through defmt, so an unexpectedly large
layer shows up as a line in the log rather than as an unexplained slow frame.

This turns Stage 4's gate into roughly forty lines of code.

### 4.3 The three things the freed SRAM pays for

**Double buffering at full band height (S5).** Two 12.8 KB buffers in SRAM
instead of one, hiding the 1.8 ms of DMA per band behind the 3.3-5.9 ms of
render, which is most of the 39 ms the SPI half costs. Most of the plumbing
exists: `hal/rp/dma.rs::start_write` is already asynchronous and `DMA_IRQ_0`
already gives a semaphore from the ISR. What blocks is
`spi/mod.rs::write_raw`, which takes the SPI lock, starts the DMA and then
immediately waits in `finish_isr_xfer!`. The work is an async display path —
a `write_pixels_async` on the `HalDisplay` trait, `lv_display_flush_ready`
called from the completion rather than from `flush_cb`, and the SPI lock held
across the transfer instead of within one call. Gate it on the display owning
its bus, which is the same predicate `touch_private_bus` already expresses for
the XPT2046 boards, where the touch controller shares SPI with the panel and
this would be unsafe.

**`LV_OBJ_STYLE_CACHE = 1` (S6).** It is `0` today, so every style property
read walks the object's style list and its parent chain, on every draw of every
object in every band. The cache costs RAM per object, and after 4.1 there is
RAM. But **A/B it before any of this work** — it is one flash cycle, and it is
the cheapest possible answer to the largest and least understood term in the
frame.

**Headroom.** ~48 KB still free after the two items above, against 12 KB now.
The budget stops being the automatic reason to reject the next idea.

### 4.4 What does not move

The FreeRTOS arena, the operand stacks, and the band buffers stay in SRAM. The
interpreter touches the heap every opcode and the renderer touches the band
buffer every pixel. The loaded class bytes remain the sensible *second* tenant,
after Stage 4 has produced a number, and they need the sub-allocator that
Stage 2 deliberately does not build.

## 5. The thing the 8 MB actually unlocks: render the page once

This is the option that gets to fluid rather than merely better, and it exists
only with PSRAM.

The Set-time screen is one 720 px page inside a scroller. At RGB565 that page is
320 x 720 x 2 = 460,800 bytes: comfortable in 8 MB, and impossible anywhere
else on this board. Render it **once** into an off-screen buffer in PSRAM, and a
scroll step stops costing any rendering at all — it becomes a window move plus
a transfer. Composed with S4's `VSCRSADD`, only the newly exposed rows go to
the panel: roughly 15,000 px, about 4 ms of SPI.

That is a different claim from S4 alone. S4 makes each frame cheaper; this makes
the render cost vanish from the steady state. It is the only path here with a
plausible route to 30 fps rather than a respectable 10-15.

What it costs, stated plainly:

- One full-page render into PSRAM on screen entry, at whatever PSRAM write
  bandwidth Stage 1 measures. Paid once per screen, not per frame — but if that
  number is bad, the screen transition becomes the new complaint.
- Reading it back is a DMA out of the XIP window. That works, but the `tCEM`
  burst limit applies and the page will not fit the cache, so every scroll
  frame is a cache sweep.
- It needs the framework to understand that a scroller can be pre-rendered.
  That is the same `ScrollView` seam S4 needs, so design them together or not
  at all.
- It is wrong for content that animates or updates mid-scroll, so it needs an
  honest invalidation path back to ordinary rendering. Getting that path wrong
  shows up as stale pixels, which is a worse bug than a slow frame.

Gate: only after S4 is built and Stage 1 has produced a real bandwidth number.

## 6. Open questions this plan does not close

- **What the per-pixel cost actually is.** 78-138 cycles/px, and two confident
  explanations have already been wrong. The style cache, XIP instruction misses
  and the blend inner loop are all still live. The RP2350's cache hit and access
  counters read across a scroll would settle the middle one directly, and that
  is the single most informative measurement left anywhere in this area.
- **Whether QMI M1 timing can be programmed without disturbing flash XIP on
  M0.** The timing registers are per-chip-select, but the clock source is
  shared. Verify on the bench, not from the datasheet.
- **How much of the 1.6 ms per-band floor is fixed overhead.** Decides whether
  the RAM-neutral 10-row double-buffer variant is a win or a wash, and therefore
  whether §4.3's first item needs PSRAM at all.
- **Whether hardware vertical scroll generalises past this one screen.** Decides
  where the seam belongs, and §5 inherits the answer.

## 7. Order

Steps 1-5 are the fluid-scrolling project. Steps 6-9 are the PSRAM project,
and it is worth starting only if step 3 says the style cache matters, or step 5
lands and the pre-rendered page becomes the next thing worth having.

| # | Step | Needs PSRAM | Effort | Buys |
|---|---|---|---|---|
| 1 | S3: `LV_DEF_REFR_PERIOD` 33 -> 16 | no | one line | the 15-30 ms of idle per frame |
| 2 | S2: honest `lv_tick_inc` | no | hours | animation and fling timing that is correct rather than approximate |
| 3 | **S6 measurement**: style-cache A/B, XIP cache counters | no | one flash cycle | decides most of what follows |
| 4 | S5 RAM-neutral variant: 10-row bands, two buffers | no | a day | answers the per-band-overhead question; hides some SPI |
| 5 | **S4: hardware vertical scroll** | no | framework feature | ~9x fewer pixels per scroll step |
| 6 | PSRAM Stages 1-3 | — | the enabling chain | nothing directly |
| 7 | Stage 4: pool to PSRAM + SRAM draw-buffer handler | yes | small, given 6 | 64 KB of SRAM back |
| 8 | S5 at full band height, and style cache on | 7 | a day | most of the 39 ms SPI half |
| 9 | Pre-rendered page in PSRAM | yes, and 5 | the largest item here | render cost leaves the steady state |

Step 3 is the one to do next, whatever else is decided. It is an afternoon, and
spending weeks on steps 6-8 to unlock a style cache that turns out not to matter
is the expensive version of this mistake.
