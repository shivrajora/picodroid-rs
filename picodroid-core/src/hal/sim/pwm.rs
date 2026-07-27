// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_pin: u8) {}

pub fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool) {
    println!("[sim] PWM GP{pin} enabled={enabled} freq={freq_hz:.1}Hz duty={duty_cycle:.1}%");
}
