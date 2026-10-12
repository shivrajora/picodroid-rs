# Handoff: bench verification left after the 2026-10-11 bug-scan fixes

The five items of `docs/bugscan-2026-10-open-items.md` were implemented and pushed on 2026-10-11
(`2ab9d250..e1910e27`, one commit per fix, pre-commit and the sim smoke green). One of them, A4,
was confirmed and verified on `pico_display2_w` the same day, and its soak on `pico_touch_kit`
(§4) ran clean that evening. Two checks remain, and both need the XPT2046 resistive panel of the
testbench family: A1 and A3. Nothing here is blocking: the fixes
are on `main` and CI is green; these runs decide whether the handoff can be closed or whether a
calibration or a counter needs a second look.

Read in order: what already happened on the bench (§1), the two runs left (§2–§3) and the one
done (§4), what to watch in the nightlies meanwhile (§5), and where to record the outcome (§6).

Before touching a board: take its lease (`./scripts/device-lock.sh acquire --board NAME`), check
`./scripts/device-lock.sh status`, and never `pkill -f probe-rs`. `flash.sh`, `pdb.sh`,
`hil-run.sh` and `parity-bench.sh --hil` take the lease themselves when given `--board`; release
with `./scripts/device-lock.sh release` when done. The testbench boards share one slot with the W
boards (`pico_enviro_mon_w` in `fleet.conf`), so a nightly or another session on any of them
holds the probe for all of them.

## 1. What the bench already showed (2026-10-11, `pico_display2_w`)

- `benchmark` passed its HIL row (`./scripts/hil-run.sh --board pico_display2_w --app benchmark
  --no-email --mode no-shrink`), 97.7 s total, no SPI diagnostic line.
- `graphicsbench` with the kind-aware predicate alone (commit `da8c1cb2`) logged
  `spi0: DMA write of 12800 bytes retired without its completion interrupt` once per run, three
  runs out of three, always in the text-and-fill section. The nightlies had logged the same event
  for a week as `spi0: transfer finished with 0 of 0 bytes read back`. Cause: a poll-gap race, not
  a lost interrupt (commit `e1910e27` explains it). After that commit, two full runs through to
  the 60 s display doze logged nothing. Scores 11,546 / 11,547 against 12,037 before, a different
  link, inside the rebuild spread.
- The display runs at the same 75 MHz as before (A2 changed the number in `board.toml`, not the
  divisor); the panel looked as it always has.

- `pico_touch_kit`, the same evening: `graphicsbench` three times through
  `parity-bench.sh --hil` (scores 12,841 / 12,842 / 12,841, identical text-fill times) and then
  `picoclock` for an hour under 799 scripted swipes and taps (`pdb input`, every 4 s, so the
  panel never dozed), RTT captured throughout: no `spi0` line of either spelling, no stall, no
  panic. The ST7796's 320×480 bands behave like the ST7789's.

So: A2 and A4 are done. A1 and A3 could not be touched on either board (no XPT2046).

## 2. A1 — the SPI ISR refill bound, under pdb load (`testbench_rp2350` or `_w`, or `testbench_rp2040`)

**What to look for.** The once-per-boot line

```text
spi1: transfer finished with 29 of 30 bytes read back (N RX overruns so far); completing it from the controller's state (further occurrences are silent)
```

is the XPT2046 poll losing a byte. Before the fix the nightlies printed it on the W slot most
days (grep `build/hil/logs/pico_enviro_mon_w/*/` for `29 of 30`). The fix bounds the ISR refill
so at most eight bytes are ever outstanding; the new `N RX overruns so far` count says whether
the FIFO was involved when the line does print. The ISR keeps the count in `RX_OVERRUNS`
(`platforms/rp/src/hal/rp/spi/mod.rs`), readable from a debugger for the rest of the run.

**Run.**

```bash
./scripts/flash.sh --board testbench_rp2350 --app qa_ui        # any touch app; keeps RTT attached
# second shell, pdb traffic on the same board for a few minutes:
while :; do ./scripts/pdb.sh --board testbench_rp2350 devices >/dev/null; sleep 0.2; done
```

Touch the panel continuously while the loop runs (slow drags, a few minutes). The touch poll is
a 30-byte full-duplex transfer at 2 MHz from the UI task; the USB ISR servicing pdb at the same
NVIC priority is the latency source the scan named.

**Outcomes.**

- No line in several minutes: A1 closed. Record it in §6.
- The line prints with `0 RX overruns so far`: the byte was not lost in the FIFO. That is the
  other suspect the ISR comment names (a receive-timeout cleared by step 5 as it asserted) or the
  poll-gap race of `e1910e27` on the ISR path; note the count and the app, leave the fix in.
- The line prints with a non-zero count: the bound is not enough on its own. Check whether the
  USB ISR priority moved (the count only grows when `RORRIS` is set on ISR entry) and bring the
  log to the next scan.

The same run on `testbench_rp2040` is worth ten minutes if the board is free: the M0+ has the
longer ISR latency.

## 3. A3 — the 12-bit XPT2046 decode and the doubled tables (one testbench board)

**What changed.** The decode now takes all twelve bits and the three `[touch]` tables in
`platforms/rp/boards/testbench_*/board.toml` were doubled (`cal_x_min 3940, cal_x_max 370,
cal_y_min 220, cal_y_max 3900`). The sim synthesises frames in the same layout and its
`keynav`/`qa_ui` rows pass, but those inject coordinates past the panel; only a finger shows
whether the doubled numbers put the edges where the panel's edges are.

**Run.** Scripted taps are not enough; use a probe that prints what the panel reports. A
ten-line app does it (apps import `picodroid.*`):

```java
public class TouchProbe extends Activity {
  @Override protected void onCreate(Bundle b) {
    super.onCreate(b);
    TextView v = new TextView();
    v.setOnTouchListener((view, e) -> {
      if (e.getAction() == MotionEvent.ACTION_DOWN)
        Log.i("touch", (int) e.getX() + "," + (int) e.getY());
      return true;
    });
    setContentView(v);   // a content root with no size is match_parent, so getX/getY are panel pixels
  }
}
```

Flash it (`./scripts/flash.sh --board testbench_rp2350 --app touchprobe` after dropping it under
`examples/`), tap the four corners and the centre, read the `touch:` lines over RTT. Alternatively
`examples/gesturedemo` shows the tap position in its status label, but relative to a 220×120
surface, so it only checks the middle of the panel.

**Expected.** Each corner within a few pixels of (0,0), (319,0), (319,239), (0,239); the centre
near (160,120). The first sample after touch-down on a resistive panel lands 20–60 px off and is
discarded by the sampler, so hold each tap a moment.

**Outcomes.**

- Within a few pixels: A3 closed.
- One axis off by a constant or scaled: the table for that axis wants a fresh capture, not a
  re-halving. `Display.calibrate()` (any app can call it) shows four crosses, reads the raw
  panel through `hal::touch::read_raw_unfiltered()` and applies the fit, but does not log it:
  for the session add a `pd_info!` of the four values at the end of
  `graphics/lvgl/calibration.rs::apply_calibration`, run it, and copy them into `board.toml`.
  Values should sit in 0..4095; anything near 2000 at a panel edge means the old units crept
  back.
- A dead strip at the far-x edge (touches there ignored): the high rejection bound (4000) is now
  live; it leaves ~40 counts past `cal_x_min = 3940`. Raise it toward 4050 in
  `crates/pd-drivers/src/xpt2046.rs` (`reject_hi`) only if the strip is real.

One board per family is enough; the three tables are identical.

## 4. A4 — the DMA band soak (`pico_touch_kit`) — done 2026-10-11

Run that evening as described below (§1 has the numbers): an hour of `picoclock` under scripted
input, plus three `graphicsbench` runs, with no fallback line. Kept here as the recipe for the
day the line reappears in a nightly (§5):

```bash
./scripts/flash.sh --board pico_touch_kit --app picoclock      # an hour, RTT attached
```

Scroll, open Set-time, let it idle and wake. Any `spi0: DMA write` or `spi0: transfer finished`
line is the finding; none after an hour closes A4 on this driver too. The scheduling monitor
(`docs/scheduling-diagnostics.md`, `--sched-diag`) shows whether a band ever waited the full
`SPI_XFER_POLL_MS`.

## 5. What the nightlies will say meanwhile

`hil-fleet.sh` (4 AM) runs the W slot and the touch kit. Two greps on
`build/hil/logs/<slot>/<run>/*.log` tell the story without a bench session:

| Signature | Before 2026-10-11 | Expected now |
| --- | --- | --- |
| `spi0: transfer finished with 0 of 0 bytes read back` | most days on the W slot | never (the message no longer exists; its successor is `spi0: DMA write of … retired`) |
| `spi0: DMA write of … retired without its completion interrupt` | n/a | never; one occurrence is a finding for §4 |
| `spi1: transfer finished with 29 of 30 bytes read back` | several days a week on the W slot | ideally never; if it prints, the `RX overruns so far` count decides §2's outcome |

`hil-email`'s new-vs-known triage will flag the reworded line as new the first time it appears.

## 6. Recording the outcome

Append a dated line per item to the table under "A. Device findings" in
`docs/bugscan-2026-10-follow-ups.md` (status column: closed, or what was seen and the log
path), and if a table was recaptured, say so in the `board.toml` comment next to the values.
When all three are closed, move §2–§4 of this document to `docs/completed/` the way the other
handoffs went.
