//! Display geometry constants for the PicoEnviroMon board (ST7789 1.14" 240x135).

pub const SCREEN_WIDTH: u16 = 240;
pub const SCREEN_HEIGHT: u16 = 135;

/// LVGL partial-render band height (rows per flush).
/// 240 * 27 * 2 = 12,960 bytes per band buffer.
pub const BAND_HEIGHT: usize = 27;

/// LVGL scroll threshold — no touch on this board; use LVGL default.
pub const SCROLL_LIMIT: u8 = 10;
