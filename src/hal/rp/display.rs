//! Display facade — delegates to the generic ST7789 driver via board config.
//!
//! Preserves the free-function API (`hal::display::init()`, etc.) so that
//! `engine.rs` and LVGL callbacks need zero changes.

use crate::boards;
use core::ptr::addr_of_mut;

pub const WIDTH: u16 = boards::SCREEN_WIDTH;
pub const HEIGHT: u16 = boards::SCREEN_HEIGHT;
pub const BAND_HEIGHT: usize = boards::BAND_HEIGHT;
pub const SCROLL_LIMIT: u8 = boards::SCROLL_LIMIT;

static mut DISPLAY: Option<boards::Display> = None;

/// Initialize the display hardware via the board-specific driver.
pub fn init() {
    unsafe {
        addr_of_mut!(DISPLAY).write(Some(boards::create_display()));
    }
}

/// Set the active drawing window.
pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
    unsafe {
        (*addr_of_mut!(DISPLAY))
            .as_mut()
            .unwrap()
            .set_window(x0, y0, x1, y1);
    }
}

/// Stream RGB565 pixel data to the display within the current window.
pub fn write_pixels(data: &[u8]) {
    unsafe {
        (*addr_of_mut!(DISPLAY))
            .as_mut()
            .unwrap()
            .write_pixels(data);
    }
}

/// No-op on hardware — only meaningful for the sim's minifb window.
#[inline(always)]
pub fn update_window() {}

/// Always true on hardware — only meaningful for the sim's minifb window.
#[inline(always)]
pub fn is_window_open() -> bool {
    true
}

/// Turn the backlight on or off.
pub fn set_backlight(on: bool) {
    unsafe {
        (*addr_of_mut!(DISPLAY)).as_mut().unwrap().set_backlight(on);
    }
}
