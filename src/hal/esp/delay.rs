// SPDX-License-Identifier: GPL-3.0-only
use embedded_hal::delay::DelayNs;

pub struct EspDelay;

impl EspDelay {
    pub fn new() -> Self {
        Self
    }
}

impl DelayNs for EspDelay {
    fn delay_ns(&mut self, _ns: u32) {}
}
