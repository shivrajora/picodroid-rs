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
