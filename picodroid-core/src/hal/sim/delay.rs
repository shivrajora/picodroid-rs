// SPDX-License-Identifier: GPL-3.0-only
//! Simulator `embedded-hal` DelayNs — no-op (instant).

use embedded_hal::delay::DelayNs;

pub struct SimDelay;

/// Public API of a library crate since stage 8, so clippy asks for the
/// `Default` pair; the type is a unit struct, so both are the same value.
impl Default for SimDelay {
    fn default() -> Self {
        Self::new()
    }
}

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
