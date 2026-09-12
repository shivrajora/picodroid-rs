# Taller draw bands on the touch board: raise `band_height` to 120

**Status: planned 2026-09-12, not started. Written as a hand-off** — it assumes
no context beyond this file. Every number in it was measured on
`pico_touch_kit` hardware on 2026-09-11 and is reproducible with the recipe in
§6. Companion to
[scroll-performance-2026-09.md](scroll-performance-2026-09.md) §5 (S9), which
records the raw results, and
[psram-lvgl-fluid-scroll-2026-09.md](psram-lvgl-fluid-scroll-2026-09.md), which
§9 here finally gives a measured reason to build.

**The change is two numbers.** Everything else in this document is why they are
the right two, how to prove it on the bench, and what to do if it goes wrong.

## 1. What "120-row bands" actually means

This is the part worth reading even if the rest looks obvious, because the name
is not self-explanatory and the mechanism is what makes the change work.

**There is no framebuffer on this board.** The panel is a 320x480 ST7796 on SPI
and the firmware never holds a full picture of it — a full frame would be
320 x 480 x 2 = 307,200 bytes against 520 KB of total SRAM. Instead LVGL runs in
`LV_DISPLAY_RENDER_MODE_PARTIAL` (`graphics/lvgl/lifecycle.rs`, in `init`), where
it is handed **one small buffer** and told to work in slices.

A slice is a full-width horizontal strip. The code calls it a *band*:

```rust
const BAND_HEIGHT: usize = hal::display::BAND_HEIGHT;      // board.toml
const BAND_BUF_SIZE: usize = hal::display::WIDTH as usize * BAND_HEIGHT * 2;
static mut BAND_BUF: BandBuf = BandBuf([0u8; BAND_BUF_SIZE]);  // .bss
```

So **`band_height` is how many pixel rows LVGL renders and ships at a time**, and
the buffer is sized from it: 320 px wide, two bytes per pixel for RGB565.

| `band_height` | Buffer | Bands in a full-viewport repaint |
|---:|---:|---:|
| 20 (today) | 12,800 B | 22 |
| 60 | 38,400 B | 8 |
| **120 (this plan)** | **76,800 B** | **4** |

The band count comes from the scrolling area, which is 436 px tall (480 minus
the 44 px header): 436 / 20 rounds up to 22, and 436 / 120 rounds up to 4.

**The loop.** For each band, LVGL walks the widget tree, and for every object
that intersects that band it resolves styles, sets up clipping, measures text and
builds draw tasks — then renders into the buffer and calls `flush_cb`, which
DMAs it to the panel over SPI. Then it does the next band.

**That is the whole reason this change works.** The per-object setup is repeated
**once per band the object crosses**. A widget spanning the viewport at 20 rows is
set up 22 times; at 120 rows, 4 times. The pixels drawn are identical either way.

The evidence that this dominates, rather than pixel work: **every band contains
exactly the same number of pixels** (320 x 20 = 6,400), yet measured band costs
within a single frame range from **1.9 ms to 11 ms**, and up to 24 ms on a screen's
first paint. A six-fold spread at constant area cannot be per-pixel cost. The
cheap bands are empty background; the expensive ones have widgets in them.

So "raise `band_height` to 120" means: *make LVGL's slices six times taller, so
it walks the widget tree four times per repaint instead of twenty-two.*

## 2. What it buys, measured

Identical scripted gesture in every row (§6), `picoclock`'s Set-time screen:

| Config | Bands | Render | Flush | Frame | fps | Entry paint |
|---|---:|---:|---:|---:|---:|---:|
| `band_height 20` (today) | 22 | 108.9 ms | 39.0 ms | 149.6 ms | 6.69 | 447 ms |
| `band_height 60` | 8 | 72.8 ms | 36.5 ms | 110.6 ms | 9.04 | 293 ms |
| **`band_height 120`** | **4** | **59.2 ms** | **35.9 ms** | **96.3 ms** | **10.39** | **248 ms** |

Against today: **render −46 %, frame time −36 %, frame rate +55 %**, and the
entry paint — the visible top-to-bottom wipe when a screen opens — nearly halves.

Two things worth noting:

- **The flush improves too** (−8 %), which was not the point. Four window-setup
  command sequences instead of twenty-two. Same pixels over the same bus.
- **It was confirmed by hand.** With the 4-band build flashed, the reported
  impression was "the scrolling feels smoother now", which also confirms the
  picture renders correctly at this band height.

**A cost model, from the three points.** Per-band setup is about 2.76 ms on this
screen and close to linear. Extrapolating to one band gives ~54 ms of render,
which is this widget tree's genuine per-pixel floor. That is why the plan stops
at 120: going to 2 bands would buy roughly 6 ms more for another 77 KB.

## 3. The change

Two values. Both files carry comments that the change makes wrong, so update
those in the same commit.

**`platforms/rp/boards/pico_touch_kit/board.toml`**, in `[display]`:

```toml
band_height = 120   # was 20
```

**`platforms/rp/mcus/rp/rp2350b.toml`**:

```toml
heap_kb = 344       # was 408
```

`heap_kb` lives in the MCU descriptor rather than board.toml, and
`pico_touch_kit` is the **only** board using `rp2350b`, so this affects no other
board. Confirm that before editing:

```bash
grep -rln 'mcu = "rp2350b"' platforms/rp/boards/
```

The comment above `heap_kb` currently explains why it is 408 ("the same as the A
variant and for the same reason"). That reasoning no longer applies to this
descriptor and the comment has to say what replaced it: the arena is funding the
draw buffer, with the measured margin from §5. Likewise the `lv_mem_kb` note in
board.toml says to raise things "only against a measured need, and re-check the
headroom the build prints" — this is that measured need, and it is worth saying
so next to `band_height` so the next person does not have to rediscover §1.

## 4. Why the arena is the funding source

The buffer lives in `.bss`, and on this board `.bss` is essentially full: 520,264
of 532,480 bytes, leaving 12,216 for the core-0 main stack against a floor of
8,192 that `scripts/lib.sh` enforces. Growing the buffer by 64,000 bytes needs
64,000 bytes from somewhere, and there are only three tenants big enough:

| Tenant | Size | Verdict |
|---|---:|---|
| FreeRTOS arena (`heap_kb`, the JVM heap) | 417,792 B | **the source** |
| LVGL pool (`LV_MEM_SIZE`) | 65,536 B | wanted, but see §9 |
| Band buffer | 12,800 B | the thing being grown |

Cutting the arena by 65,536 B funds 64,000 B of buffer with 1,536 B left over.
The expected build output, which is the gate to watch:

```
RAM:   518,784 / 532,480 bytes (97%)
Main stack headroom: 13,696 bytes (floor 8192)
```

Total RAM goes **down** slightly and headroom goes **up**, because the arena cut
slightly exceeds the buffer growth. If the headroom line comes out below 8,192
the build fails, which is the intended safety net.

## 5. What the arena cut costs, measured

`pdb sysmon` reports the arena's all-time low-water mark. After driving the same
gesture:

| Arena | Network | Lowest free | Peak use |
|---|---|---:|---:|
| 352,256 B (`heap_kb 344`) | down | 76,712 B | 275,544 B |
| 393,216 B (`heap_kb 384`) | **up, IP assigned** | 116,896 B | 276,320 B |

`picoclock` peaks around 276 KB, so a 344 KB arena leaves roughly **76 KB of
margin, about 22 %**.

**Associating with WiFi costs essentially no arena.** Peak use differs by under
800 bytes between radio-down and radio-up-with-an-IP, so the network stack's
buffers are not coming from it. This was expected to be the main risk of cutting
the arena on a W board and the measurement says it is not one. Do not re-derive
this by reasoning; it was tested.

## 6. How to verify on the bench

The board is `pico_touch_kit`. Take its lease implicitly by naming it
(`--board pico_touch_kit`); see the bench section of the root `CLAUDE.md`.

**Build and flash.** `flash.sh` does *not* read `.wifi-creds.env`, so export the
credentials if you want the radio up:

```bash
set -a; . ./.wifi-creds.env; set +a
./scripts/flash.sh --board pico_touch_kit --app picoclock
```

Run that in the background: it tails RTT and never exits. When you are done with
a build, **stop that task before flashing again** — a leftover `probe-rs` holds
the USB interface and the next flash fails with "Failed to open probe", whose
message unhelpfully blames udev rules.

**The probe.** Put this in `graphics/lvgl/lifecycle.rs` and revert it afterwards;
it does not pass clippy and is not meant to be committed. In `tick`, reset
counters before `lv_timer_handler()` and report after it; in `flush_cb`, charge
the gap since the previous flush to render and the `write_pixels` call to flush.
Timestamps come from `hal::system_clock::elapsed_realtime_nanos()`. One line per
painted frame, giving band count, render µs, flush µs and frame µs.

**Drive it from the host, not by hand**, so every variant sees the same gesture:

```bash
./scripts/pdb.sh --board pico_touch_kit input tap 236 440     # the "Set time" button
sleep 3
./scripts/pdb.sh --board pico_touch_kit input swipe 160 400 160 150 400
```

`pdb` talks over USB CDC, so it works alongside the RTT session. The tap
coordinate is the centre of the right-hand button on the clock face
(`Ui.columnX(1, 2)` = 164, width 144, `buttonsY` = 412, height 56).

**Read the arena** with `./scripts/pdb.sh --board pico_touch_kit sysmon`, which
prints free and min-free heap.

**What good looks like:** 4 bands per full repaint, render around 59 ms, frame
around 96 ms, and a sysmon min-free above 70 KB with the radio up.

## 7. Risks, and what would make you back off

- **Heap margin for apps other than `picoclock`.** 76 KB of headroom was measured
  with one app on one screen. The settings app's Storage screen already needed a
  larger background pool once, and installing a package allocates. Before
  landing, drive the launcher, the settings app including Storage, and an install,
  then re-read `sysmon`. If min-free drops under ~40 KB, take the 60-row variant
  instead (§8).
- **The `heap_kb` comment claims parity with the A variant.** Changing it means
  the two RP2350 descriptors no longer build identically, which the comment
  treats as a property worth keeping. It is a documentation fix, not a blocker,
  but say so explicitly rather than silently diverging.
- **Nothing else on the bench is affected**, but verify rather than trust: only
  `pico_touch_kit` uses `rp2350b`, and the binary-size ratchet measures
  `testbench_rp2040` and `testbench_rp2350`, neither of which does.
- **Tearing may look different.** Fewer, larger band writes change where seams
  fall. Nothing mitigates tearing today either way (see §6 of the scroll doc), so
  this is a "look at it" item, not a gate.

## 8. The fallback: 60 rows

If §7's heap sweep is unhappy, `band_height = 60` with `heap_kb = 384` is a
measured 9.04 fps for a 24 KB cut instead of 64 KB, keeping about 114 KB of
margin with the radio up. It captures most of the win for a third of the
exposure, and is a one-value change from this plan.

## 9. The follow-on that gives the heap back

The LVGL pool is 65,536 bytes and a 120-row buffer needs 64,000 more than a
20-row one. **Moving the pool to PSRAM funds this buffer almost exactly, with
1,536 bytes to spare, and the JVM arena never has to shrink at all.**

That is now the strongest argument for the PSRAM work in
[psram-lvgl-fluid-scroll-2026-09.md](psram-lvgl-fluid-scroll-2026-09.md), and a
better one than the double-buffering case that originally motivated it: the
payoff is a measured +55 % frame rate rather than an enabling change with a
speculative one. Note also that plan's §4.1 — moving the pool is a single
`LV_MEM_ADR` define, because the vendored allocator already supports it.

Whoever lands the arena cut should leave a pointer to it in that plan, so the
PSRAM work knows it has a heap debt to repay.

## 10. What this does not fix

Stated so the next session does not over-claim it.

- **Scrolling is still not smooth.** 10.4 fps is better than 6.7 and it is
  noticeable, but the structural fix is S4, hardware vertical scroll on the
  ST7796, which cuts a scroll step from 139,520 pixels to about 15,000. That
  remains the biggest remaining lever and this change composes with it.
- **The entry wipe is still 248 ms**, and roughly half of what remains is the
  `DatePicker` on that screen: hiding it took the entry paint from 447 ms to
  244 ms while changing steady scrolling not at all. See §5 (S6b) of the scroll
  doc.
- **Nothing here touches tearing.**

## 11. Checklist

1. `grep -rln 'mcu = "rp2350b"' platforms/rp/boards/` — confirm one board.
2. Edit the two values in §3; rewrite both stale comments.
3. Build and watch for `Main stack headroom: 13,696 bytes (floor 8192)`.
4. Flash with WiFi credentials exported; confirm `net: up, ip …`.
5. Run the §6 gesture; expect 4 bands, ~59 ms render, ~96 ms frame.
6. Sweep the launcher, settings including Storage, and an install; read `sysmon`.
7. Revert the probe. Confirm `git status` shows only the intended files.
8. `./scripts/pre-commit`, then `./scripts/pre-commit --full` before pushing.
9. Record the result in §5 of the scroll doc and leave the §9 pointer.
