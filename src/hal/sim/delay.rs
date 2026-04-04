//! Simulator `embedded-hal` DelayNs — no-op (instant).

use embedded_hal::delay::DelayNs;

pub struct SimDelay;

impl SimDelay {
    pub fn new() -> Self {
        Self
    }
}

impl DelayNs for SimDelay {
    fn delay_ns(&mut self, _ns: u32) {
        // No-op in sim — don't slow down host tests.
    }
}
