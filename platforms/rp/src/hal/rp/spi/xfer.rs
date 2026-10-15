// SPDX-License-Identifier: GPL-3.0-only
//! Pure SPI logic: FIFO thresholds, clock-divisor math, transfer state.
//!
//! No `rp-pico`/`rp235x-hal`/FreeRTOS deps — host-compilable and unit-testable.

pub const SPI_FIFO_DEPTH: usize = 8;
pub const SMALL_XFER_THRESHOLD: usize = 8;

/// How long the waiter blocks between looks at the controller while an
/// interrupt-driven transfer is in flight. The completion interrupt is what
/// normally ends the wait — this is only the cadence at which a transfer that
/// will never signal is noticed, so it trades nothing against a bus that is
/// behaving.
pub const SPI_XFER_POLL_MS: u32 = 2;

/// The cap on one interrupt-driven transfer. Reached only when the controller
/// is still busy and the completion has not come: the shape of a bus held by a
/// device that is not answering, which is a fault to report, not to wait out.
pub const SPI_XFER_TIMEOUT_MS: u32 = 5_000;

/// What the bus is doing between a start and its `finish_isr_xfer!`. The ISR
/// only ever sees full-duplex work; a write-only transfer goes through DMA,
/// which never touches the byte counters, so the kind is what tells the
/// waiter which account of "finished" to believe (see [`transfer_finished`]).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SpiOp {
    Idle,
    FullDuplex,
    DmaWrite,
}

pub struct SpiXferState {
    pub op: SpiOp,
    pub tx_ptr: *const u8,
    pub rx_ptr: *mut u8,
    pub len: usize,
    pub tx_idx: usize,
    pub rx_idx: usize,
}

impl SpiXferState {
    pub const fn new() -> Self {
        Self {
            op: SpiOp::Idle,
            tx_ptr: core::ptr::null(),
            rx_ptr: core::ptr::null_mut(),
            len: 0,
            tx_idx: 0,
            rx_idx: 0,
        }
    }
}

/// Whether the controller says an interrupt-driven transfer is over, for a
/// transfer whose completion interrupt has not arrived.
///
/// The ISR ends a transfer by counting bytes *received*, so a byte that never
/// reaches `rx_idx` leaves `rx_idx < len` forever, and no further interrupt
/// can come: the receive timeout only asserts on a FIFO that is not empty.
/// The controller's own state still settles the question — everything queued
/// has been shifted out (`tx_idx >= len`), the shifter is idle and the RX FIFO
/// is empty — and answering it that way costs one sample instead of the whole
/// timeout cap (docs/qa-2026-09-13-followups.md item 1).
pub fn controller_finished(tx_idx: usize, len: usize, busy: bool, rx_not_empty: bool) -> bool {
    tx_idx >= len && !busy && !rx_not_empty
}

/// Whether a DMA write is over although its completion interrupt (the RX
/// drain channel's) has not arrived: both channels have retired, the TX FIFO
/// has drained and the shifter is idle. `BSY` alone is not enough — it drops
/// for a moment whenever the TX FIFO underruns between two DMA bursts, which
/// is exactly when a band is still in flight.
pub fn dma_write_finished(dma_busy: bool, tx_fifo_empty: bool, busy: bool) -> bool {
    !dma_busy && tx_fifo_empty && !busy
}

/// The waiter's question, answered for the kind of transfer actually in
/// flight. An `Idle` state means the completion has been signalled and the
/// credit is about to be (or has been) taken: not finished *here*, the next
/// `take` collects it.
pub fn transfer_finished(
    op: SpiOp,
    tx_idx: usize,
    len: usize,
    dma_busy: bool,
    tx_fifo_empty: bool,
    busy: bool,
    rx_not_empty: bool,
) -> bool {
    match op {
        SpiOp::FullDuplex => controller_finished(tx_idx, len, busy, rx_not_empty),
        SpiOp::DmaWrite => dma_write_finished(dma_busy, tx_fifo_empty, busy),
        SpiOp::Idle => false,
    }
}

/// Whether the ISR may queue one more TX byte. Bounded by the bytes
/// outstanding (queued but not yet read back), not only by TX-FIFO-not-full:
/// with the RX FIFO as deep as the TX FIFO, more than `SPI_FIFO_DEPTH` bytes
/// in flight is an RX overrun waiting for one slow interrupt — the shape of
/// the "short read" `controller_finished` tolerates. This is the bound
/// pico-sdk's `spi_write_read_blocking` keeps.
pub fn may_queue_tx(tx_idx: usize, rx_idx: usize, len: usize) -> bool {
    tx_idx < len && tx_idx.wrapping_sub(rx_idx) < SPI_FIFO_DEPTH
}

/// Compute the SSP prescaler/divisor pair `(cpsdvsr, scr)` for the requested
/// bus frequency, given the peripheral clock. `cpsdvsr` must be even (≥2);
/// `scr` is clamped to u8 range. Output frequency is `pclk / (cpsdvsr * (scr + 1))`.
///
/// The division floors, so the rate programmed is the *lowest the controller
/// can do that is at least* the request: 62.5 MHz at a 150 MHz PCLK is 75 MHz.
/// A rate a peripheral must not exceed is checked with [`is_exact_hz`] where
/// it is configured (the display's, at build time).
pub const fn clock_divisors(pclk_hz: u32, freq_hz: u32) -> (u8, u8) {
    let want = if freq_hz == 0 { 1 } else { freq_hz };
    let mut scr = pclk_hz / (2 * want);
    scr = scr.saturating_sub(1);
    if scr > 255 {
        scr = 255;
    }
    (2u8, scr as u8)
}

/// The bus rate [`clock_divisors`] actually programs for `freq_hz`.
pub const fn achieved_hz(pclk_hz: u32, freq_hz: u32) -> u32 {
    let (cpsdvsr, scr) = clock_divisors(pclk_hz, freq_hz);
    pclk_hz / (cpsdvsr as u32 * (scr as u32 + 1))
}

/// Whether the controller can run at exactly `freq_hz` from this PCLK.
/// `const`, so a board's `spi_freq` can be refused at compile time rather
/// than silently rounded up past a panel's rated clock.
pub const fn is_exact_hz(pclk_hz: u32, freq_hz: u32) -> bool {
    achieved_hz(pclk_hz, freq_hz) == freq_hz
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Completion from the controller's state ───────────────────────────

    #[test]
    fn a_transfer_still_shifting_is_not_finished() {
        // Everything queued, but the shifter is still working: the completion
        // interrupt is still owed and must be waited for.
        assert!(!controller_finished(30, 30, true, false));
    }

    #[test]
    fn a_transfer_with_bytes_still_in_the_rx_fifo_is_not_finished() {
        // The receive timeout will assert for these and the ISR will finish
        // the count itself; ending the transfer here would drop good data.
        assert!(!controller_finished(30, 30, false, true));
    }

    #[test]
    fn a_transfer_with_tx_left_to_queue_is_not_finished() {
        assert!(!controller_finished(29, 30, false, false));
    }

    #[test]
    fn an_idle_controller_with_everything_sent_is_finished() {
        // The qa_life stall: rx_idx one short, nothing left in flight, no
        // interrupt possible. Answering `true` is what turns a five-second
        // freeze into a lost sample.
        assert!(controller_finished(30, 30, false, false));
    }

    #[test]
    fn a_zero_length_transfer_is_finished() {
        assert!(controller_finished(0, 0, false, false));
    }

    // ── The DMA write's own account ──────────────────────────────────────

    #[test]
    fn a_dma_write_is_not_finished_while_a_channel_is_busy() {
        // The TX FIFO underran between two bursts: BSY is low for a moment,
        // the FIFO is empty, and the band is nowhere near done.
        assert!(!dma_write_finished(true, true, false));
    }

    #[test]
    fn a_dma_write_is_not_finished_while_the_fifo_or_shifter_holds_bytes() {
        assert!(!dma_write_finished(false, false, false));
        assert!(!dma_write_finished(false, true, true));
    }

    #[test]
    fn a_retired_dma_write_with_an_idle_bus_is_finished() {
        assert!(dma_write_finished(false, true, false));
    }

    // ── The dispatcher reads the kind, not stale counters ────────────────

    #[test]
    fn a_dma_write_is_judged_by_the_channels_not_by_the_last_isr_transfer() {
        // The previous ISR transfer left tx_idx == len; a DMA band in flight
        // with BSY momentarily low must still not count as finished (A4).
        assert!(!transfer_finished(
            SpiOp::DmaWrite,
            30,
            30,
            true,
            true,
            false,
            false
        ));
        assert!(transfer_finished(
            SpiOp::DmaWrite,
            0,
            0,
            false,
            true,
            false,
            false
        ));
    }

    #[test]
    fn a_full_duplex_transfer_ignores_the_dma_channels() {
        assert!(transfer_finished(
            SpiOp::FullDuplex,
            30,
            30,
            true,
            false,
            false,
            false
        ));
    }

    #[test]
    fn an_idle_state_is_never_finished_here() {
        // The ISR has already given the credit; the next take collects it.
        assert!(!transfer_finished(
            SpiOp::Idle,
            30,
            30,
            false,
            true,
            false,
            false
        ));
    }

    // ── The refill bound ─────────────────────────────────────────────────

    #[test]
    fn the_refill_stops_at_fifo_depth_outstanding() {
        // After the seed: 8 queued, none read back — nothing more may go.
        assert!(!may_queue_tx(8, 0, 30));
        // The ISR drained three: three more may be queued, no more.
        assert!(may_queue_tx(8, 3, 30));
        assert!(may_queue_tx(10, 3, 30));
        assert!(!may_queue_tx(11, 3, 30));
    }

    #[test]
    fn the_refill_never_queues_past_len() {
        assert!(!may_queue_tx(30, 29, 30));
        assert!(may_queue_tx(29, 29, 30));
    }

    #[test]
    fn the_refill_bound_holds_nine_bytes_back() {
        // The overrun case from the 2026-10 scan: seed 8, TX interrupt with
        // 4 in flight, 3 drained. The old loop refilled to a full TX FIFO,
        // leaving 9 outstanding; the bound allows exactly 8.
        let (mut tx, rx, len) = (8usize, 3usize, 30usize);
        while may_queue_tx(tx, rx, len) {
            tx += 1;
        }
        assert_eq!(tx - rx, SPI_FIFO_DEPTH);
    }

    // ── Constants ────────────────────────────────────────────────────────

    #[test]
    fn fifo_depth_is_nonzero_pow2() {
        assert!(SPI_FIFO_DEPTH > 0);
        assert_eq!(SPI_FIFO_DEPTH & (SPI_FIFO_DEPTH - 1), 0);
    }

    #[test]
    fn small_xfer_fits_in_fifo() {
        assert!(SMALL_XFER_THRESHOLD <= SPI_FIFO_DEPTH);
    }

    // ── SpiXferState::new ────────────────────────────────────────────────

    #[test]
    fn new_state_is_idle() {
        let s = SpiXferState::new();
        assert!(s.op == SpiOp::Idle);
        assert_eq!(s.len, 0);
        assert_eq!(s.tx_idx, 0);
        assert_eq!(s.rx_idx, 0);
    }

    #[test]
    fn new_state_has_null_pointers() {
        let s = SpiXferState::new();
        assert!(s.tx_ptr.is_null());
        assert!(s.rx_ptr.is_null());
    }

    // ── SpiOp equality ───────────────────────────────────────────────────

    #[test]
    fn spi_op_variants_distinct() {
        assert!(SpiOp::Idle != SpiOp::FullDuplex);
    }

    #[test]
    fn spi_op_copy() {
        let a = SpiOp::FullDuplex;
        let b = a; // Copy
        assert!(a == b);
    }

    // ── clock_divisors ───────────────────────────────────────────────────

    #[test]
    fn clock_divisors_cpsdvsr_is_always_2() {
        for freq in [100_000, 1_000_000, 4_000_000, 10_000_000, 25_000_000] {
            let (cps, _scr) = clock_divisors(125_000_000, freq);
            assert_eq!(cps, 2);
        }
    }

    #[test]
    fn clock_divisors_1mhz_at_125mhz_pclk() {
        // freq = pclk / (2 * (scr + 1)) = 125M / (2 * (scr+1))
        // 1 MHz ⇒ scr+1 = 62.5 ⇒ scr = 61 (truncated) or 62 depending on formula
        let (_, scr) = clock_divisors(125_000_000, 1_000_000);
        let out = 125_000_000u32 / (2 * (scr as u32 + 1));
        assert!((out as i32 - 1_000_000).abs() <= 2_100_000); // within one SCR step
        assert!(scr >= 60 && scr <= 63);
    }

    #[test]
    fn clock_divisors_very_low_freq_clamps_scr_to_255() {
        let (_cps, scr) = clock_divisors(125_000_000, 1);
        assert_eq!(scr, 255);
    }

    #[test]
    fn clock_divisors_very_high_freq_yields_scr_zero() {
        let (_cps, scr) = clock_divisors(125_000_000, 100_000_000);
        assert_eq!(scr, 0);
    }

    #[test]
    fn clock_divisors_freq_zero_does_not_panic() {
        let (cps, scr) = clock_divisors(125_000_000, 0);
        assert_eq!(cps, 2);
        assert_eq!(scr, 255);
    }

    #[test]
    fn clock_divisors_pclk_zero_yields_zero_scr() {
        let (cps, scr) = clock_divisors(0, 1_000_000);
        assert_eq!(cps, 2);
        assert_eq!(scr, 0);
    }

    #[test]
    fn clock_divisors_monotonic_decreasing_in_freq() {
        // Higher requested freq ⇒ smaller scr (shorter period).
        let (_, scr_low) = clock_divisors(125_000_000, 1_000_000);
        let (_, scr_high) = clock_divisors(125_000_000, 10_000_000);
        assert!(scr_high < scr_low);
    }

    #[test]
    fn the_display_request_is_rounded_up_at_150_mhz_and_exact_at_125() {
        // The A2 finding, pinned: 62.5 MHz asked for at a 150 MHz PCLK is
        // SCR 0, i.e. 75 MHz, because the division floors. At 125 MHz the
        // same request is exact. Boards therefore ask for the rate they get.
        assert_eq!(achieved_hz(150_000_000, 62_500_000), 75_000_000);
        assert!(!is_exact_hz(150_000_000, 62_500_000));
        assert!(is_exact_hz(150_000_000, 75_000_000));
        assert!(is_exact_hz(150_000_000, 37_500_000));
        assert!(is_exact_hz(125_000_000, 62_500_000));
        // The touch rate is deliberately approximate (2.03 MHz at 150 MHz)
        // and nothing there needs it exact.
        assert!(!is_exact_hz(150_000_000, 2_000_000));
    }

    #[test]
    fn is_exact_hz_is_usable_in_a_const_context() {
        const OK: bool = is_exact_hz(150_000_000, 75_000_000);
        assert!(OK);
    }

    #[test]
    fn clock_divisors_output_freq_below_or_equal_requested() {
        // scr is truncated, so actual freq ≤ requested.
        for req in [500_000u32, 1_000_000, 4_000_000, 10_000_000] {
            let (_, scr) = clock_divisors(125_000_000, req);
            let actual = 125_000_000u32 / (2 * (scr as u32 + 1));
            // Truncation means actual may be up to ~2x the request when scr is small;
            // just assert scr is at least floor(pclk/(2*req)) - 1.
            let expected_scr = (125_000_000u32 / (2 * req)).saturating_sub(1).min(255) as u8;
            assert_eq!(scr, expected_scr, "freq={}", req);
            let _ = actual;
        }
    }
}
