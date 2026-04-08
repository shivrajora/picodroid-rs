//! Display geometry constants for the TestBench board (Waveshare 2.8" ST7789).

pub const SCREEN_WIDTH: u16 = 320;
pub const SCREEN_HEIGHT: u16 = 240;

/// LVGL partial-render band height (rows per flush).
/// 320 * 20 * 2 = 12,800 bytes per band buffer.
pub const BAND_HEIGHT: usize = 20;

/// LVGL scroll threshold (pixels) — raised from default (10) to compensate
/// for XPT2046 resistive touchscreen jitter (~5 px between settled frames).
pub const SCROLL_LIMIT: u8 = 30;
