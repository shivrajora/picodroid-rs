// SPDX-License-Identifier: GPL-3.0-only
// No-op display stubs for ESP sim/test builds (no minifb window in M1).

mod cfg {
    include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
}
pub use cfg::BAND_HEIGHT;
pub use cfg::SCREEN_HEIGHT as HEIGHT;
pub use cfg::SCREEN_WIDTH as WIDTH;
pub use cfg::SCROLL_LIMIT;

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
