//! PicoEnviroMon board — RP2350 with ST7789 1.14" 240x135 display, no touch.
//!
//! Pin mapping: TBD (placeholder values — fill in when hardware arrives).

mod display_config;
pub use display_config::*;

use crate::drivers::st7789::St7789;
use crate::hal::delay::RpDelay;
use crate::hal::output_pin::RpOutputPin;
use crate::hal::spi_bus::RpSpiBus;

// TODO: confirm SPI ID and frequency for this board
const SPI_ID: u8 = 1;
const DISPLAY_SPI_FREQ: u32 = 62_500_000;

// TODO: confirm pin assignments for this board
const PIN_DC: u8 = 8;
const PIN_CS: u8 = 9;
const PIN_RST: u8 = 15;
const PIN_BL: u8 = 13;

// --- Concrete types for this board ---
pub type Display = St7789<RpSpiBus, RpOutputPin, RpOutputPin, RpOutputPin, RpOutputPin, RpDelay>;

/// Create and initialize the ST7789 display driver for this board.
pub fn create_display() -> Display {
    let spi = RpSpiBus::new_init(SPI_ID, DISPLAY_SPI_FREQ);
    let dc = RpOutputPin::new(PIN_DC, false);
    let cs = RpOutputPin::new(PIN_CS, true);
    let rst = RpOutputPin::new(PIN_RST, false);
    let bl = RpOutputPin::new(PIN_BL, false);
    let delay = RpDelay::new();

    // TODO: confirm MADCTL for this panel orientation
    let mut display = St7789::new(
        spi,
        dc,
        cs,
        rst,
        bl,
        delay,
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        0x00,
    );
    display.init();
    display
}
