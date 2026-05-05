// SPDX-License-Identifier: GPL-3.0-only
//! ESP32-S3 display stub — Milestone 1.
//! Real ST7789 via esp-hal SPI wiring is Milestone 4.

mod board_display {
    include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
}
pub use board_display::BAND_HEIGHT;
pub use board_display::SCREEN_HEIGHT as HEIGHT;
pub use board_display::SCREEN_WIDTH as WIDTH;
pub use board_display::SCROLL_LIMIT;

pub fn init() {}
pub fn set_window(_x0: u16, _y0: u16, _x1: u16, _y1: u16) {}
pub fn write_pixels(_data: &[u8]) {}
pub fn set_backlight(_on: bool) {}
pub fn display_sleep() {}
pub fn display_wake() {}
pub fn update_window() {}
pub fn is_window_open() -> bool {
    true
}
