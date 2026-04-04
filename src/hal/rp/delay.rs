//! `embedded-hal` DelayNs wrapper using `cortex_m::asm::delay`.

use embedded_hal::delay::DelayNs;

/// Busy-wait delay using the Cortex-M cycle counter.
pub struct RpDelay;

impl RpDelay {
    pub fn new() -> Self {
        Self
    }
}

// Cycles per nanosecond at 150 MHz ≈ 0.15, so ns / 7 ≈ cycles.
// At 125 MHz (RP2040) this is slightly conservative (slower delays), which is safe.
#[cfg(feature = "chip-rp2350")]
const NS_PER_CYCLE: u32 = 7; // 150 MHz: ~6.67 ns/cycle, rounded to 7
#[cfg(feature = "chip-rp2040")]
const NS_PER_CYCLE: u32 = 8; // 125 MHz: 8 ns/cycle

impl DelayNs for RpDelay {
    fn delay_ns(&mut self, ns: u32) {
        cortex_m::asm::delay(ns / NS_PER_CYCLE);
    }
}
