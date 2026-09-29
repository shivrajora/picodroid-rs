# Fluid scrolling on the touch board: the PSRAM plan, and what LVGL buys with the room

**Status: Stages 1–4 built 2026-09-12, §4.1 measured and left off.** The
part is up and spot-checked at every boot (`hal/rp/psram.rs`, 8 MB detected,
a full sweep behind the `psram-sweep` feature came back clean twice), the
region and keys exist, the XIP rule is in the porting guide and in
`with_xip_disabled!`, and `lv_mem_in_psram` moves the pool with one key. The
measurement §3's Stage 4 demanded: the pool in PSRAM costs 10 ms of a 98 ms
frame and 45 ms of a 205 ms entry paint on the Set-time screen, with layers
already served from SRAM — see the 2026-09-12 note in §1 below and §4.1 in
[completed/psram-lvgl-fluid-scroll-2026-09.md](../completed/psram-lvgl-fluid-scroll-2026-09.md). Every
step of §7 up to 7 is therefore done, and 7 is priced.

Completed items: [completed/psram-lvgl-fluid-scroll-2026-09.md](../completed/psram-lvgl-fluid-scroll-2026-09.md) — Stages 1–3 (§3), §4.1 pool move (Stage 4), §4.2 SRAM draw-buffer handler, §6 QMI M1 open question (answered), §7 steps 1–8.

The implementation plan for the
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

> **2026-09-11, measured: that 64 KB now has a price tag.** Raising the draw
> band from 20 rows to 120 cuts frame time 36 % and lifts the frame rate 55 %,
> and it needs 64,000 B — almost exactly the pool. Today the only way to fund
> it is cutting the JVM arena, which is what
> [band-height-120-2026-09.md](band-height-120-2026-09.md) does as an interim.
> Moving the pool here repays that debt and costs the JVM nothing. This is now
> the strongest reason to build §3, ahead of the double-buffering case below —
> which §5 of [scroll-performance-2026-09.md](scroll-performance-2026-09.md)
> also refutes in its RAM-neutral form.

> **2026-09-12, built and measured: it costs the JVM nothing and the frame
> 10 %.** Same firmware, same gesture, radio up — pool in `.bss` with a 344 KB
> arena: 61.6 ms render, 98.3 ms frame, 205 ms entry paint; pool in PSRAM with
> a 408 KB arena: 71.7 ms, 109.5 ms, 250 ms. The §4.2 fix ([completed](../completed/psram-lvgl-fluid-scroll-2026-09.md)) was in place and no
> layer ever fell back to the pool, so this is not the layers. It is the
> draw-task churn: `lv_draw_add_task` and every `lv_draw_*` allocate, fill and
> free a task and a descriptor in the pool for each draw call in each band,
> and that traffic now crosses the QSPI bus through the 16 KB XIP cache the
> code also runs from. `pico_touch_kit` ships the key off and keeps the arena
> cut; the numbers sit beside the key in its board.toml. Two untested ways to
> take the 10 ms back, if the heap is ever wanted more than the frame: pin the
> TLSF control block (about 3 KB) in the cache with the RP2350's
> pin-at-address maintenance op, or give draw tasks an SRAM slab of their own,
> which means patching `lv_draw.c` in the vendored LVGL.

## 2. Fluid scrolling does not need PSRAM

Worth stating before planning three stages that buy nothing on their own. The
two largest wins in the profiling need no memory at all:

- **S4, hardware vertical scroll** (`VSCRDEF`/`VSCRSADD` on the ST7796) cuts a
  scroll step from 139,520 px to about 15,000. Zero RAM. Neither command
  appears in `drivers/st7796.rs` today.
- **S2 and S3**, an honest `lv_tick_inc` and a 16 ms refresh period, recover
  roughly 15 % of the frame. Zero RAM. **S3 has landed**, measured: every paint
  waited three ticks and now waits one (§4 of the scroll doc).
- **S5's cheap variant**, two 6.4 KB buffers at a 10-row band height instead of
  one 12.8 KB buffer at 20 rows, is RAM-neutral.

So PSRAM is not on the critical path. It is what makes the *good* versions of
S5 and S6 affordable, and it is the only thing that makes §5 possible at all.
Sequence it accordingly: §7 puts the free wins and the one decisive measurement
ahead of every stage below.

## 3. Bringing PSRAM up

Three stages, in this order, none of which produces a user-visible change.

All three are built (2026-09-12); Stages 1–3 are in
[completed/psram-lvgl-fluid-scroll-2026-09.md](../completed/psram-lvgl-fluid-scroll-2026-09.md).

## 4. What LVGL does with the 64 KB

§4.1 (the pool move, Stage 4) and §4.2 (the SRAM draw-buffer handler) are built
and measured; they are in
[completed/psram-lvgl-fluid-scroll-2026-09.md](../completed/psram-lvgl-fluid-scroll-2026-09.md).

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
  number is bad, the screen transition becomes the new complaint. *Measured:*
  8.7 MB/s through the cache, so the 460,800-byte page is about 53 ms of bus
  time on top of the render; reading it back at 19.4 MB/s is 24 ms for the
  whole page and under 2 ms for a 47 px step.
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

## 7. Order

Steps 1–8 are done or superseded and are in
[completed/psram-lvgl-fluid-scroll-2026-09.md](../completed/psram-lvgl-fluid-scroll-2026-09.md); step 9 is what
remains. Steps 1-5 are the fluid-scrolling project. Steps 6-9 are the PSRAM project,
and it is worth starting only if step 3 says the style cache matters, or step 5
lands and the pre-rendered page becomes the next thing worth having.

| # | Step | Needs PSRAM | Effort | Buys |
|---|---|---|---|---|
| 9 | Pre-rendered page in PSRAM | yes, and 5 | the largest item here | render cost leaves the steady state |

Step 3 is the one to do next, whatever else is decided. It is an afternoon, and
spending weeks on steps 6-8 to unlock a style cache that turns out not to matter
is the expensive version of this mistake.

## Amendments

### 2026-09-16 — the §7 order table, re-read

The table in §7 predates most of what it orders. Row by row, against
`09e7a8b3`:

| # | Step | State |
|---|---|---|
| 1 | S3 | done (2026-09-11) |
| 2 | S2: honest `lv_tick_inc` | **done 2026-09-15** as scheduling-audit WP7 (`d8563ae3` + `9d090232`); not yet measured on the touch kit |
| 3 | S6 measurement | done 2026-09-11 — no hot spot, style cache no gain (scroll doc §5) |
| 4 | S5 RAM-neutral variant | superseded by row 8 |
| 5 | S4 hardware vertical scroll | done 2026-09-12 |
| 6 | PSRAM Stages 1–3 | done 2026-09-12 |
| 7 | Stage 4: pool to PSRAM + SRAM draw-buffer handler | built 2026-09-12 (`lv_draw_buf_sram.c`); measured at 10 % of the frame, so `lv_mem_in_psram = false` on `pico_touch_kit` |
| 8 | S5 async flush + double buffering | done 2026-09-12 — two 60-row buffers funded from the arena, not from PSRAM |
| 9 | Pre-rendered page in PSRAM | **open**, not started |

"Step 3 is the one to do next" is void. What is open from this plan: step 9,
Stage 5 (loaded class bytes in PSRAM, which needs the sub-allocator Stage 2 did
not build), and giving the PSRAM a tenant that pays — the pool was the first
candidate and lost. The frame-cost items that do not need PSRAM are tracked in
the scroll doc's 2026-09-16 amendment.
