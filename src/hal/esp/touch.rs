// SPDX-License-Identifier: GPL-3.0-only
// ESP32-S3 touch stub — Milestone 1. Real XPT2046 via SPI is Milestone 4.

pub fn init() {}

pub fn read_point() -> Option<(u16, u16)> {
    None
}

pub fn read_raw_unfiltered() -> (u16, u16) {
    (0, 0)
}

pub fn set_calibration(_cal_x_min: u16, _cal_x_max: u16, _cal_y_min: u16, _cal_y_max: u16) {}
