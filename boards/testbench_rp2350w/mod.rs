// TestBench (WiFi) board — Waveshare 2.8" Pico display (ST7789 + XPT2046)
// on a Raspberry Pi Pico 2 W (RP2350 + CYW43439).
//
// Display/touch config is in board.toml. Only WiFi pin constants remain here
// because the CYW43 driver references them directly.

pub const CYW43_PIN_WL_ON: u8 = 23;
pub const CYW43_PIN_WL_D: u8 = 24;
pub const CYW43_PIN_WL_CS: u8 = 25;
pub const CYW43_PIN_WL_CLK: u8 = 29;
