# Handoff: the bug-scan items that still need the bench

Everything host-testable from the 2026-10-10 scan is fixed and on `main` as of `54f9a219`
(2026-10-11; the per-item record is `docs/bugscan-2026-10-follow-ups.md`). The five items this
document used to hand over — four RP-family hardware findings and the packer's modified-UTF-8
refusal — were implemented on 2026-10-11 (second pass, uncommitted at the time of writing; see the
follow-ups record). What is left is the bench verification each item asked for, which only the
boards with the affected part can give. Each entry below says what changed, where, and how to know
it is good. The bench runs themselves, with recipes and expected signatures, are
`docs/bench-handoff-2026-10-11.md`.

Before touching a board: take its lease (`./scripts/device-lock.sh acquire --board NAME`), check
`./scripts/device-lock.sh status`, and never `pkill -f probe-rs`. The `flash.sh`, `pdb.sh` and
`hil-run.sh` scripts take the lease themselves when given `--board`.

## A1. The SPI ISR refilled nine bytes against an eight-deep RX FIFO — fixed, bench pending

**What changed.** `platforms/rp/src/hal/rp/spi/mod.rs`: step 3 of the ISR refills while
`xfer::may_queue_tx(tx_idx, rx_idx, len)` (`tx_idx - rx_idx < SPI_FIFO_DEPTH`) and TNF, the bound
pico-sdk's `spi_write_read_blocking` keeps, so at most eight bytes are ever outstanding. A new
step 0 reads `SSPRIS.RORRIS` before anything clears it and counts into `RX_OVERRUNS`; the
once-per-boot short-read line now prints that count, so a fallback that fires is attributable (a
short read with the count at 0 is not the FIFO losing a byte). Unit tests in `spi/xfer.rs`
(`the_refill_*`) pin the bound, including the nine-byte case from the scan.

**Verify.** `testbench_rp2350` (XPT2046 touch on SPI, polled from the UI task) with pdb traffic
running: flash any touch app (`./scripts/flash.sh --board testbench_rp2350 --app qa_ui`), then in
a second shell drive `./scripts/pdb.sh --board testbench_rp2350 ...` commands in a loop (a
`devices` or `ls` every 200 ms) while touching. Before the fix the RTT log showed the
once-per-boot short-read line early; after it the line should not appear for a session of several
minutes, and if it does, the overrun count in it says whether the FIFO was involved.

## A2. The display SPI clock was programmed at 75 MHz when 62.5 MHz was asked for — fixed (option b)

**What changed.** The six RP2350 boards' `[display] spi_freq` (`pico_enviro_mon`,
`pico_enviro_mon_w`, `pico_display2_w`, `pico_touch_kit`, `testbench_rp2350` and
`testbench_rp2350w`) now say `75000000`, the rate they have always run at; `testbench_rp2040`
(PCLK 125 MHz, where 62.5 MHz is exact) is unchanged. `spi/xfer.rs` gained `const fn`
`achieved_hz` / `is_exact_hz`, and `hal/rp/display.rs` has a `const` assert that refuses a
`spi_freq` the chip cannot program exactly, so a rounded-up panel clock cannot recur silently. The
touch rate (2 MHz, 2.03 MHz programmed) is deliberately not checked. No bus rate changed, so no
page-turn timing moved.

**Verify.** Nothing to measure: the programmed divisor is identical before and after. Flash any
RP2350 board and look at the panel; a `./scripts/parity-bench.sh --hil --board pico_touch_kit`
(graphicsbench) run should match the last one. The 75 MHz overclock of the ST7789V's 16 ns write
cycle is now a documented choice (`docs/designs/scroll-performance-2026-09.md` §9); the day a
panel shows artifacts, the legal rates below it at this PCLK are 37.5 and 25 MHz.

## A3. XPT2046 samples were decoded as 11-bit values — fixed, bench pending

**What changed.** `crates/pd-drivers/src/xpt2046.rs::decode` takes bits 14..3 of the two result
bytes (`((b1 << 8 | b2) >> 3) & 0x0FFF`), the chip's layout; the six `cal_*` values of
`testbench_rp2040`, `testbench_rp2350` and `testbench_rp2350w` were doubled (`3940 / 370 /
220 / 3900`), the driver's low noise-rejection bound went from 100 to 200 (the same threshold in
the new units; the high bound of 4000 was already in 12-bit units and now has effect), and the
`FakeBus` test plus the sim's `FakeXptSpi` (`picodroid-core/src/hal/sim/touch.rs`) encode the
chip's layout. A calibration written down before 2026-10-11 is in the old, halved units.

**Verify.** Touch the four corners and the centre by hand on one testbench board and check the
`[touch]` log coordinates land within a few pixels of the panel edges; `qa_ui` / `keynav` via the
sim-ctrl taps are not enough (they inject coordinates past the driver). One board per family is
enough; the three tables are identical. If an edge is dead by a few pixels, recalibrate with the
raw coordinates logged rather than re-halving anything.

## A4. `finish_isr_xfer!` judged a DMA write by the previous ISR transfer's state — fixed, confirmed and verified on `pico_display2_w`

**What changed.** `SpiOp` has a `DmaWrite` kind; `write_raw_start` records it (and resets the
byte counters) before `dma::start_write`, and the wait loop asks `xfer::transfer_finished`, which
dispatches on the kind: the ISR path keeps `controller_finished`, the DMA path needs both DMA
channels retired (`dma::busy`), the TX FIFO empty (`TFE`) and the shifter idle, and an `Idle`
kind is never "finished here" (the credit is about to be taken). `finish_isr_xfer!` sets the kind
back to `Idle` on every exit. The once-per-boot line distinguishes the two kinds. Unit tests in
`spi/xfer.rs` (`a_dma_write_*`, `*_judged_by_the_channels_*`) cover the predicate.

**What the bench showed (2026-10-11, `pico_display2_w`).** With the kind-aware predicate alone,
`graphicsbench` logged the new "DMA write of 12800 bytes retired without its completion
interrupt" line once per run, three runs out of three, always during the text-and-fill section.
The nightlies had been logging the same event for a week as `spi0: transfer finished with 0 of 0
bytes read back` (the stale, empty ISR state), so the suspicion was real. The cause is a gap, not
a lost interrupt: a 12.8 KB band at 75 MHz takes 1.4 ms, the waiter's `take` is two ticks and can
expire after one, and a band that ends between the timeout and the register read has already
given its credit, which the fallback then threw away as stale. The wait now tries a zero-length
`take` before believing the fallback; with that, two full `graphicsbench` runs (through to the
60 s display doze) logged nothing, with a score of 11,546 against 12,037 before (a different
link; within the rebuild spread `docs/perf-campaign-2026-08.md` measured). The remaining question is only whether the line ever appears again; `picoclock` on
`pico_touch_kit` over an hour is the soak that would settle it.

## C10c. Modified UTF-8 (CESU-8) in class and member names — supported

**What changed.** `tools/class-shrink/src/classfile.rs` gained `modified_utf8_to_utf8` and
`ClassFile::normalize_utf8`, which re-encode every constant-pool `Utf8` entry (surrogate pairs to
one four-byte sequence, the two-byte NUL to a NUL) and leave a malformed entry alone. `papk-pack`
runs it on every class it collects, whichever shrink mode is on, and `shrink-dir` runs it before
matching names against the map. An ASCII class comes back byte for byte (the golden
`minimal.papk` test still holds). `class-link` still refuses what is not UTF-8 after that, which
is now a malformed name rather than a non-ASCII one. Tests: `class-shrink` (`modified_utf8_*`,
`normalize_*`) and `papk-pack` (`a_class_name_with_a_supplementary_character_packs`, which also
checks the linker still refuses the unconverted bytes).

**Consequence worth knowing.** String literals were re-encoded too: a literal with an emoji used to
reach the runtime without the `LIT_UTF8` flag and resolve to nothing; it is now a UTF-8 byte
string under the usual string model (`length()` counts its four bytes).

## Noted, not bugs

- `ToggleButton` keeps its on/off texts in two 31-byte slots
  (`graphics/lvgl/widgets/toggle_button.rs::TEXT_BUF_SIZE`); longer texts are cut. Every other
  widget text native takes any length since 2026-10-11.
- Under the UTF-8 string model `toCharArray()` hands bytes back while `StringBuilder.append(char)`
  encodes a char from U+0080 as UTF-8, so a `toCharArray → append` round trip of non-ASCII text
  does not reproduce the string. Documented in `api/core.md`; changing it means deciding what a
  `char` is for `charAt`/`length` too.
