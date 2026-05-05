// SPDX-License-Identifier: GPL-3.0-only
use embedded_hal::delay::DelayNs;

pub struct Delay;

impl DelayNs for Delay {
    fn delay_ns(&mut self, ns: u32) {
        std::thread::sleep(std::time::Duration::from_nanos(ns as u64));
    }
}
