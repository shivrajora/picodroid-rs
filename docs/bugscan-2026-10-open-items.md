# Handoff: the bug-scan items that still need the bench

Everything host-testable from the 2026-10-10 scan is fixed and on `main` as of `54f9a219`
(2026-10-11; the per-item record is `docs/bugscan-2026-10-follow-ups.md`). Five items remain. Four
are RP-family hardware findings that a sim cannot reproduce or verify; one is a packer design
question. Each entry below says what is wrong, where, how to see it, what the fix is, and how to
know the fix is good. Nothing here is started: no branch, no partial code.

Before touching a board: take its lease (`./scripts/device-lock.sh acquire --board NAME`), check
`./scripts/device-lock.sh status`, and never `pkill -f probe-rs`. The `flash.sh`, `pdb.sh` and
`hil-run.sh` scripts take the lease themselves when given `--board`.

## A1. The SPI ISR refills nine bytes against an eight-deep RX FIFO

**Severity.** Medium. It is the shape of the "short read" the driver already tolerates, so the
symptom is a wasted poll (one sample lost, a log line once per boot), not a hang. Under USB load
it can fire on every touch poll.

**Where.** `platforms/rp/src/hal/rp/spi/mod.rs`, the ISR macro: step 3 ("Refill TX FIFO") loops
while `tx_idx < len && TNF`, i.e. bounded only by TX-FIFO-not-full; step 5 clears RORIC/RTIC
(`sspicr = 0x03`) without reading RORIS first. The tolerant fallback is `controller_finished` in
`spi/xfer.rs`, used from the wait loop in `mod.rs` (`SHORT_READ_REPORTED`).

**Why it overruns.** After the 8-byte seed the TX interrupt asserts with four bytes still in
flight. The ISR drains three RX bytes and refills TX to full, so nine bytes are outstanding
against an eight-deep RX FIFO. Any ISR latency of five byte-times or more loses one byte; the USB
ISR at the same NVIC priority writing a 64-byte packet byte by byte is enough. Step 5 then erases
the evidence.

**Fix.** Bound the refill by outstanding bytes, as pico-sdk's `spi_write_read_blocking` does:
refill while `state.tx_idx - state.rx_idx < SPI_FIFO_DEPTH` (the constant already exists) in
addition to TNF. Optionally read `sspris().roris()` before the clear in step 5 and count overruns
into a diagnostic counter next to `SHORT_READ_REPORTED`, so the fallback's firing becomes
attributable.

**Verify.** `testbench_rp2350` (XPT2046 touch on SPI, polled from the UI task) with pdb traffic
running: flash any touch app (`./scripts/flash.sh --board testbench_rp2350 --app qa_ui`), then in
a second shell drive `./scripts/pdb.sh --board testbench_rp2350 ...` commands in a loop (a
`devices` or `ls` every 200 ms) while touching. Before the fix the RTT log shows the once-per-boot
short-read line early; after the fix it should not appear for a session of several minutes, and
the overrun counter (if added) should stay at 0. The unit tests in `spi/xfer.rs` cover
`controller_finished` only; add one for the refill bound if the bound is factored into a pure
function.

## A2. The display SPI clock is programmed at 75 MHz when 62.5 MHz is asked for

**Severity.** Medium, and a product decision rather than a code defect. The ST7789V's write cycle
is 16 ns (62.5 MHz); the panels run at 75 MHz today and visibly work.

**Where.** `platforms/rp/src/hal/rp/spi/xfer.rs::clock_divisors`:
`scr = pclk / (2 * freq) - 1` floors, which picks the divisor giving at least the requested rate.
At PCLK 150 MHz (every RP2350 board) `spi_freq = 62500000` gives SCR 0, i.e. 75 MHz. The boards
that set it: `pico_enviro_mon`, `pico_enviro_mon_w`, `pico_display2_w`, `pico_touch_kit`,
`testbench_rp2350` (`[display] spi_freq` in each `board.toml`). On `testbench_rp2040` PCLK is
125 MHz, so the same formula gives 62.5 MHz exactly; it is not affected. The unit test next to
`clock_divisors` re-derives the same formula and does not pin the intent.

**Options.** (a) Round up the divisor (`div_ceil`), which drops those displays to 37.5 MHz, the
only legal rate below 75 at this PCLK (75, 37.5, 25). Expect a visible page-turn slowdown; the
2026-09 scroll-performance work found render and SPI time split roughly 2:1, so SPI time would
about double. (b) Keep 75 MHz and make the configuration honest: set the five `spi_freq` values to
`75000000` and change `clock_divisors` to refuse (build-time panic) a frequency it cannot hit
exactly, so this cannot recur silently. (c) Leave as is and document the overclock.

**Recommendation.** (b), unless a panel shows artifacts. The panels have run at 75 MHz through
every nightly since the boards were brought up.

**Verify.** `./scripts/parity-bench.sh --hil --board pico_touch_kit` (graphicsbench) before and
after, and a `./scripts/sim-shot.sh`-style capture is not useful here; look at the panel. If (a)
is chosen, re-measure `picoclock` page turns on the touch kit and update
`docs/designs/ram-headroom`'s fade timing if it regresses.

## A3. XPT2046 samples are decoded as 11-bit values

**Severity.** Low. Everything is consistent today: the decode discards the LSB and every board's
calibration table was captured against the halved range, so nothing mis-maps. It costs one bit of
touch resolution and makes the `cal_*` numbers mysterious to anyone reading the datasheet.

**Where.** `crates/pd-drivers/src/xpt2046.rs::decode`: `(frame[1] << 4) | (frame[2] >> 4)` is
the 12-bit sample shifted right by one. The 12-bit decode is `((frame[1] as u16) << 8 | frame[2]
as u16) >> 3 & 0x0FFF`. The calibration tables are in `[touch]` of `testbench_rp2040`,
`testbench_rp2350` and `testbench_rp2350w` (`cal_x_min = 1970`, `cal_x_max = 185`, and the y
pair). `pico_touch_kit` uses a GT911 over I2C and is unaffected. The `FakeBus` test in
`xpt2046.rs` encodes the current layout and must change with it.

**Fix.** Change `decode`, double the six `cal_*` values (or recalibrate: tap the four corners
with the raw coordinates logged), update the FakeBus expectations.

**Verify.** `qa_ui` and `keynav` on `testbench_rp2350` via the sim-ctrl taps are not enough (they
inject coordinates past the driver); touch the four corners and the centre by hand and check the
`[touch]` log coordinates land within a few pixels of the panel edges. One board per family is
enough; the three tables are identical.

## A4. `finish_isr_xfer!` judges a DMA write by the previous ISR transfer's state

**Severity.** Suspected, not reproduced. If real, the effect is an aborted display band
(a torn frame) under a momentary TX underrun.

**Where.** `platforms/rp/src/hal/rp/spi/mod.rs`, the wait loop around `controller_finished(...)`
(the `done.take(SPI_XFER_POLL_MS)` loop). `dma::start_write` never touches `SpiXferState`, so on
the DMA band path `st.tx_idx >= st.len` describes the previous ISR transfer and the predicate
degenerates to `!BSY && !RNE` sampled at the poll instant.

**Fix.** Gate the `controller_finished` branch on the transfer kind (the ISR path only), or for
the DMA path test the DMA channel's busy bit and the TX FIFO empty flag instead of BSY alone.
Either way make the kind explicit in `SpiXferState` so the predicate cannot read stale state.

**Verify.** `graphicsbench` and `picoclock` on `pico_touch_kit` for an hour with the
short-read line watched: if the branch ever fires on a DMA band the log says so once per boot.
Absence of the line after the change is the evidence. The scheduling monitor
(`docs/scheduling-diagnostics.md`) shows whether a band ever waited the full `SPI_XFER_POLL_MS`.

## C10c. Modified UTF-8 (CESU-8) in class and member names is refused at pack time

**Severity.** Low. A Java identifier containing a supplementary character (an emoji in a class or
method name) cannot be packed; the packer reports `name is not valid UTF-8`. No in-tree app does
this.

**Where.** `crates/class-link/src/classfile.rs::check_utf8`, which the link derivation applies to
every class, member and descriptor name. The runtime reads those names as `&str` straight from
flash without re-checking, which is why the builder refuses them rather than passing them on.

**Fix.** Re-encode in the packer: `class-shrink` already rewrites constant pools
(`tools/class-shrink/src/shrink.rs`), so a pass that converts each Utf8 entry from modified UTF-8
(surrogate pairs, the two-byte NUL) to standard UTF-8 before linking is the natural place. Member
names must then be re-encoded consistently in every class that references them, which the
shrinker's rename machinery does. Add a `papk-pack` test with a class whose name holds a
supplementary character.

**Decision needed.** Whether to support this at all. The alternative is to keep the refusal and
make the error message say "supplementary characters in names are not supported".

## Noted, not bugs

- `ToggleButton` keeps its on/off texts in two 31-byte slots
  (`graphics/lvgl/widgets/toggle_button.rs::TEXT_BUF_SIZE`); longer texts are cut. Every other
  widget text native takes any length since 2026-10-11.
- Under the UTF-8 string model `toCharArray()` hands bytes back while `StringBuilder.append(char)`
  encodes a char from U+0080 as UTF-8, so a `toCharArray → append` round trip of non-ASCII text
  does not reproduce the string. Documented in `api/core.md`; changing it means deciding what a
  `char` is for `charAt`/`length` too.
