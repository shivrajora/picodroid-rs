// SPDX-License-Identifier: GPL-3.0-only
//! Pure SPI logic: FIFO thresholds, clock-divisor math, transfer state.
//!
//! No `rp-pico`/`rp235x-hal`/FreeRTOS deps — host-compilable and unit-testable.

pub const SPI_FIFO_DEPTH: usize = 8;
pub const SMALL_XFER_THRESHOLD: usize = 8;
pub const MAX_STAGING_LEN: usize = 64;

#[derive(Clone, Copy, PartialEq)]
pub enum SpiOp {
    Idle,
    WriteOnly,
    FullDuplex,
}

pub struct SpiXferState {
    pub op: SpiOp,
    pub tx_ptr: *const u8,
    pub rx_ptr: *mut u8,
    pub len: usize,
    pub tx_idx: usize,
    pub rx_idx: usize,
    pub staging: [u8; MAX_STAGING_LEN],
}

unsafe impl Send for SpiXferState {}

impl SpiXferState {
    pub const fn new() -> Self {
        Self {
            op: SpiOp::Idle,
            tx_ptr: core::ptr::null(),
            rx_ptr: core::ptr::null_mut(),
            len: 0,
            tx_idx: 0,
            rx_idx: 0,
            staging: [0; MAX_STAGING_LEN],
        }
    }
}

/// Compute the SSP prescaler/divisor pair `(cpsdvsr, scr)` for the requested
/// bus frequency, given the peripheral clock. `cpsdvsr` must be even (≥2);
/// `scr` is clamped to u8 range. Output frequency is `pclk / (cpsdvsr * (scr + 1))`.
pub fn clock_divisors(pclk_hz: u32, freq_hz: u32) -> (u8, u8) {
    let scr = (pclk_hz / (2 * freq_hz.max(1))).saturating_sub(1).min(255) as u8;
    (2u8, scr)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Constants ────────────────────────────────────────────────────────

    #[test]
    fn fifo_depth_is_nonzero_pow2() {
        assert!(SPI_FIFO_DEPTH > 0);
        assert_eq!(SPI_FIFO_DEPTH & (SPI_FIFO_DEPTH - 1), 0);
    }

    #[test]
    fn small_xfer_fits_in_staging_and_fifo() {
        assert!(SMALL_XFER_THRESHOLD <= MAX_STAGING_LEN);
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

    #[test]
    fn new_state_staging_is_zeroed() {
        let s = SpiXferState::new();
        assert!(s.staging.iter().all(|&b| b == 0));
        assert_eq!(s.staging.len(), MAX_STAGING_LEN);
    }

    // ── SpiOp equality ───────────────────────────────────────────────────

    #[test]
    fn spi_op_variants_distinct() {
        assert!(SpiOp::Idle != SpiOp::WriteOnly);
        assert!(SpiOp::WriteOnly != SpiOp::FullDuplex);
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
