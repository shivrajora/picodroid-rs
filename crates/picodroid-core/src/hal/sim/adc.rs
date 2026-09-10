// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_pin: u8) {}

pub fn read(pin: u8) -> f64 {
    println!("[sim] ADC GP{pin} read → 1.65V (mid-scale)");
    1.65
}
