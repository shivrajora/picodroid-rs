# Rendering straight into RGB565_SWAPPED: handover

**Status: built 2026-09-16.** Scoped and landed the same day, right after the
LVGL v9.6.0 bump (`97635c48`), which had kept the old swap on purpose. Every code
fact below was checked against that tree, and line numbers are from it too. The
body is the plan as written. §7 records what happened when it ran.

## 1. The job in one paragraph

Today LVGL renders little-endian RGB565, and then `lv_refr.c` byte-swaps each band
in place just before it calls our `flush_cb`, because the panels want big-endian
bytes. `LV_COLOR_16_SWAP 1` turns that on. v9.6.0 deprecates it, and **v10 removes
it**. The replacement is to render natively into `LV_COLOR_FORMAT_RGB565_SWAPPED`, so
the bytes come out big-endian with no extra pass. The bytes `flush_cb` gets should
not change at all. That is the whole premise, and the checks in §5 are built to
prove it.

## 2. Why do it, and why it is not urgent

**It unblocks LVGL v10.** v9.6.0 is the last v9 release. We can't take v10 until
this is done, so do it before that bump, not as part of it.

**Don't do it for speed.** The v9.6 commit message and the comment in `lv_conf.h`
call this a win for the scroll path. The measurement says otherwise. §2 of
[scroll-performance-2026-09.md](scroll-performance-2026-09.md) ruled the swap out:
*"It is an unrolled 32-bit loop: ~3 ms per frame"*, out of a ~109 ms frame on
`pico_touch_kit`. The real saving is also smaller than 3 ms, because some of the
swap work just moves (§4.2). That number predates S4/S5, which cut a scroll frame
to ~24 ms. The swap scales with pixels flushed, and so does most of the render, so
its share has probably not changed much. Measure again before quoting any number.

When this lands, fix the comment above `LV_COLOR_16_SWAP` in
`crates/picodroid-core/lvgl/lv_conf.h`. It oversells the speed gain.

## 3. Where things stand

| Piece | Where | Today |
|---|---|---|
| Colour format | `crates/picodroid-core/lvgl/lv_conf.h` | `LV_COLOR_FORMAT_DEFAULT LV_COLOR_FORMAT_RGB565` |
| The swap | same file | `LV_COLOR_16_SWAP 1`, plus `LV_COLOR_16_SWAP_DISABLE_WARNING 1` to hide v9.6's `#warning` |
| Swapped blenders | same file | `LV_DRAW_SW_SUPPORT_RGB565_SWAPPED 0` (pinned off in the v9.6 bump, because v9.6 turns it on when there's no Kconfig) |
| Where the swap runs | `third_party/lvgl/src/core/lv_refr.c:1462-1478` | `lv_draw_rgb565_swap` over the band, in place, just before `flush_cb`. We use the `PARTIAL` branch. |
| Consumer | `crates/picodroid-core/src/graphics/lvgl/lifecycle.rs:199` `flush_cb` | Takes big-endian RGB565 and passes it to `hal::display::write_pixels_start`, or to `hw_scroll::flush` on `hw_vscroll` boards |
| Panels | `drivers/st7789.rs`, `drivers/st7796.rs` | Big-endian RGB565 over SPI |
| Sim | `hal/sim/display.rs:165-181` | Reads each pixel with `u16::from_be_bytes`, then converts to ARGB |
| Assets | `tools/papk-pack` | Every image is baked as `LV_COLOR_FORMAT_RGB565` (`0x12`), with a guard test |

None of our own code swaps pixels. Everything after `flush_cb` only needs "big-endian
bytes", and the change keeps that true.

## 4. The change

### 4.1 Config

```c
#define LV_COLOR_FORMAT_DEFAULT LV_COLOR_FORMAT_RGB565_SWAPPED
/* delete LV_COLOR_16_SWAP and LV_COLOR_16_SWAP_DISABLE_WARNING */

#define LV_DRAW_SW_SUPPORT_RGB565_SWAPPED 1   /* now the render target */
#define LV_DRAW_SW_SUPPORT_RGB565         1   /* stays: see below */
```

`LV_DRAW_SW_SUPPORT_RGB565` has to stay on even though nothing renders into RGB565
any more. The swapped blender only accepts an RGB565 **source** image (our papk
assets) inside `#if LV_DRAW_SW_SUPPORT_RGB565`
(`src/draw/sw/blend/lv_draw_sw_blend_to_rgb565_swapped.c:472`). The same switch also
pulls in the RGB565 target blender. You can't keep one without the other, so flash
will grow (§6).

That should be the whole code change. If you end up editing `flush_cb`, a driver or
the sim display, that means the "same bytes" premise is broken. Stop and find out
why before you go on.

### 4.2 What LVGL does differently (checked in v9.6.0 source)

- **The display layer** takes `disp->color_format` (`src/display/lv_display.c:109`),
  so the band buffer becomes swapped. It's still 2 bytes per pixel, so
  `BAND_BUF_SIZE` doesn't change.
- **Fills and gradients** compute the swapped colour once, so they cost nothing
  extra. This is where the saving comes from.
- **Glyphs (A8 masks)** blend straight into the swapped buffer.
- **RGB565 images** go through `rgb565_image_blend` in the swapped blender, which
  swaps each image pixel. Today that's a copy followed by the band swap, so on
  image-heavy screens the cost moves rather than disappears.
- **Scaled or rotated `ImageView`** (the transform path) turns RGB565 into
  RGB565A8 (`src/draw/sw/lv_draw_sw_img.c:495`). That gets blended "as RGB565 +
  mask" (`:536`), which ends up in the same RGB565-source case.
- **ARGB8888 layers**: the bar indicator (`lv_bar.c:648`) and the scale label
  layer ask for ARGB8888 explicitly. The swapped blender handles that source.
- **Image recolour** (`lv_obj_set_style_image_recolor`, which `ImageView` exposes)
  on an RGB565 source runs the normal RGB565 branch, then blends as above.

### 4.3 Keep assets as RGB565

Baking RGB565_SWAPPED assets in papk-pack would turn image blits back into plain
copies. **Don't do that in this change.** It changes the papk asset format and its
guard test, and on devices, installed papks would render with the wrong byte order.
There's also a branch in `lv_draw_sw_img.c:858-880` (recolour of a *swapped source*)
that looks wrong on a first read: it writes `lv_color_to_u16(color)` without
swapping, and reads channels with RGB565 shifts. Swapped assets would reach it. This
is **not verified**, so read that code closely before anyone considers swapped
assets. Leave it as a possible follow-up, after the numbers in §5.4 show image blits
are worth optimising.

## 5. How to prove it

Do these in order. Each step can stop the work.

### 5.1 Flash size first, since it can block the change

The swapped blender adds code and nothing gets removed (§4.1). The rp2040 program
region has a hard 896 K cap, and headroom was ~40 KB in June and has been spent
since. Build the rp2040 and rp2350 device images before and after, and compare the
`Flash:` lines. Then run the size ratchet (`./scripts/pre-commit --full`). If rp2040
doesn't fit, stop and decide what to do next. Don't reach for LTO; it makes this
image bigger.

### 5.2 Byte-identical output in the sim (the main proof)

The `parity-fbhash` feature logs a CRC32 of every band at the `flush_cb` seam, as
`fbhash: x1,y1,x2,y2 crc`. It hashes the big-endian bytes *before* the sim converts
them (G1 in [../parity-audit.md](../parity-audit.md)). If this change is correct, the
sequence is **identical** before and after. No script builds this feature yet. Take
the sim build line from `scripts/parity-bench.sh:232-235` and swap `parity-metrics`
for `parity-fbhash`:

```bash
B=pico_touch_kit                       # also: testbench_rp2350, pico_enviro_mon
APP=graphicsbench                      # fixed-step tick, so a fixed frame count
./scripts/build-apk.sh --app $APP
PICODROID_APK_PATH=sim-runtime cargo build --release -q \
  --manifest-path platforms/rp/Cargo.toml -p picodroid \
  --target x86_64-unknown-linux-gnu --no-default-features \
  --features "sim,board-${B//_/-},parity-fbhash"
PICODROID_APK_PATH=build/apks/$APP.papk PICODROID_SIM_HEADLESS=1 \
  timeout 600 target/x86_64-unknown-linux-gnu/release/picodroid \
  | grep '^fbhash:' > fbhash.$B.$APP.before
```

Capture on current main, apply §4.1, capture again, then `diff`. Run it on
`graphicsbench` and `qa_ui`, on all three boards. `pico_touch_kit` covers
`hw_vscroll` and `draw_buffers = 2`, `pico_enviro_mon` covers keypad focus styling,
and `testbench_rp2350` is the CI board.

**If a band differs, look at it.** Don't write it off as rounding. The swapped
blender is separate code from "RGB565 blender + swap", and a real rounding
difference (one LSB) and a real bug both show up as a different CRC. Dump the
differing band from both builds and compare the pixels. An LSB difference in blended
areas only might be acceptable. Record it here if you accept it. A wrong channel
never is.

`graphicsbench` may run without a display window. Also run `qa_ui` with a window
once and look at it: the sim's ARGB conversion is outside the hash (DSP-05), and a
swap bug shows up as obviously wrong colours.

### 5.3 On hardware

1. Flash `pico_touch_kit` (ST7796, `hw_vscroll`, two draw buffers, async DMA
   flush). This board exercises the most. Scroll a long page, and open an
   `ImageView` with scaling and recolour. Wrong byte order is obvious on the
   panel: blue and red look wrong, and whites turn pastel.
2. Flash an ST7789 board (`testbench_rp2350` or `pico_enviro_mon`) and repeat the
   colour check.
3. Optional but cheap: build firmware with `parity-fbhash` and compare the RTT
   `fbhash:` lines with the sim's for the same app. G1 says they match by
   construction, and this checks that again after the change.
4. Take the bench lease the normal way (`--board NAME`). Nothing about this work
   needs a pinned lease.

### 5.4 Speed (only to record it, not to justify the change)

Use the frame-time method from scroll-performance-2026-09.md on `pico_touch_kit`,
A/B, same session, several runs each. Device wall-clock varies about ±4% between
rebuilds, and bench placement adds more. Record three screens: a text-and-fill
screen (expect a small gain), an image-heavy one (expect no change or slightly
worse, §4.2), and a scroll. Put the numbers in §7 of this doc, tied to their
commits.

## 6. Risks

| Risk | Likelihood | What catches it |
|---|---|---|
| rp2040 goes over its flash cap | Real. The swapped blender is added code and nothing is removed | §5.1, CI's rp2040 build |
| Band bytes change (rounding or a blender bug) | Low to medium | §5.2 diff |
| Image-heavy screens get slower | Medium, small effect | §5.4 |
| Something outside `flush_cb` assumed LE pixels | Low. `grep` for `rgb565`/`from_be_bytes` finds only the sim display, and `hw_scroll.rs:208` slices rows without reading pixels | §5.2 and §5.3 |
| `lv_draw_buf_sram.c` (PSRAM hook) depends on format | None. It ignores `color_format` (`:38`) | — |
| Someone bakes swapped assets and hits the §4.3 branch | Only if §4.3 is ignored | Code review |

## 7. Results

Measured on the tree at `97635c48`, before and after the §4.1 change. The change is
exactly §4.1, and nothing outside `lv_conf.h` needed touching except three comments
that still described the old swap (`hal/sim/display.rs`, `tools/papk-pack`,
`docs/parity-audit.md` DSP-01).

### 7.1 Flash (§5.1): +6.5 KB everywhere, fits

| Image | Before | After | Delta |
|---|---|---|---|
| `testbench_rp2040` debug | 851,229 | 857,781 | +6,552 B (59.5 KB left of 917,248) |
| `testbench_rp2040` release | 830,052 | 836,604 | +6,552 B |
| `testbench_rp2350` release | 1,006,680 | 1,013,112 | +6,432 B |
| `pico_touch_kit` release | 1,325,148 | 1,331,580 | +6,432 B |

No `#warning` is left in any of the four builds.

Accepted into `bench/parity/ratchet.toml` together with growth that the LVGL
v9.6.0 bump (`97635c48`) brought in and nobody had accepted: against the previous
baseline (`22b4e6b8`) the release images grew +11,596 B (rp2040) and +11,488 B
(rp2350) flash, and +28 B RAM. This change accounts for 6,552 / 6,432 B of that.
The v9.6.0 bump accounts for the other 5,044 / 5,056 B and all 28 B of RAM. No
other firmware-affecting commit landed in between.

### 7.2 Sim band bytes (§5.2): identical

`graphicsbench` gives the same fbhash sequence in two runs of the same binary, so
the before/after diff is meaningful. It came out **identical** on all three boards:
`pico_touch_kit` (1,701 bands), `testbench_rp2350` (2,442), `pico_enviro_mon`
(1,776). `testbench_rp2040` was only captured after the change, for §7.3. Its
sequence is the same as `testbench_rp2350`'s (same 320x240 panel config).

`qa_ui` is **not** usable for a byte diff: two runs of the same binary differ in
6–14 bands. The ones inspected are an animated widget whose x position depends on
wall-clock time, plus two full-width text bands. The comparison had to be filtered
to bands that matched in both baseline runs, and even then `testbench_rp2350`
showed 5 "mismatches". Four more runs of each binary showed both binaries
producing the other's hashes for those bands (e.g. `0,100,319,119`: `2f1a33c1`
and `beb11996` from both), so the difference is timing, not rendering. `qa_ui`
passes (`=== ALL PASSED ===`) on all three boards after the change.

### 7.3 On hardware (§5.3)

Band bytes from the device over RTT (`PICODROID_EXTRA_FEATURES=parity-fbhash
./scripts/flash.sh --board <b> --app graphicsbench`), compared with the sim's
after-change sequence for the same board config. Only the lines before
`GraphicsBench: === PASSED ===` count, because the launcher draws after that:

| Board config | Physical board | Bands | Device vs sim |
|---|---|---|---|
| `testbench_rp2350` | Pico 2 W in slot `pico_enviro_mon_w` | 2,442 | **identical** |
| `testbench_rp2040` | RP2040 testbench | 2,442 | **identical** |
| `pico_touch_kit` | Pico touch kit (ST7796, `hw_vscroll`, two draw buffers) | 1,701 | **identical** |

This is stronger than a look at the panel. The sim's before and after sequences
match, and the device matches the sim, so device output is byte-identical to the
old build without flashing the old build. The touch kit's slot was leased elsewhere when the change was first built and came
free later the same day. That run went through the ST7796 driver with `hw_vscroll`
and `draw_buffers = 2` compiled in. `graphicsbench` does not scroll, but a scroll
changes nothing about byte order: the hash is taken on the band before it reaches
`hw_scroll::flush`, and `hw_scroll.rs` / `hw_vscroll.c` move rows without reading
pixel values. **Not done:** a visual colour check on any panel. The
byte match makes a wrong colour impossible unless the panel driver itself changed,
and it didn't.

The RP2040 image with fbhash compiled in is 864,031 / 917,248 B, so the
checksum build fits too. With fbhash on, `graphicsbench`'s scores on the device mean nothing: every band
goes out over RTT, so `text_fill` fell to 5 fps on the RP2350 and 3 fps on the RP2040. Don't compare these runs with
benchmark history.

### 7.4 Speed (§5.4): not measured

Not measured, because §2 already put the ceiling at ~3 ms/frame and the touch kit
was unavailable. Measure on the touch kit if anyone ever wants the number.

## 8. Sources

- LVGL migration guide: `third_party/lvgl/docs/src/changelog/migration-v9-6.mdx`,
  section "LV_COLOR_16_SWAP".
- The v9.6 bump: commit `97635c48`.
- The earlier swap measurement: [scroll-performance-2026-09.md](scroll-performance-2026-09.md) §2 and §S6.
- Why fbhash can prove this: [../parity-audit.md](../parity-audit.md) DSP-01, DSP-05, G1.
