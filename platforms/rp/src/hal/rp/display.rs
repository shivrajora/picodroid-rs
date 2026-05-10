// SPDX-License-Identifier: GPL-3.0-only
//! Display facade — delegates to the generic ST7789 driver via board config.
//!
//! Display constants and pin mappings come from the build.rs-generated
//! `display_config.rs`, driven by the `[display]` section in board.toml.

mod generated {
    include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
}

pub const WIDTH: u16 = generated::SCREEN_WIDTH;
pub const HEIGHT: u16 = generated::SCREEN_HEIGHT;
pub const BAND_HEIGHT: usize = generated::BAND_HEIGHT;
pub const SCROLL_LIMIT: u8 = generated::SCROLL_LIMIT;

#[cfg(has_display)]
mod inner {
    use super::generated;
    use crate::drivers::st7789::St7789;
    use crate::hal::delay::RpDelay;
    use crate::hal::output_pin::RpOutputPin;
    use crate::hal::spi_bus::RpSpiBus;
    use core::ptr::addr_of_mut;

    type Display = St7789<RpSpiBus, RpOutputPin, RpOutputPin, RpOutputPin, RpOutputPin, RpDelay>;

    static mut DISPLAY: Option<Display> = None;

    pub fn init() {
        let spi = RpSpiBus::new_init_with_pins(
            generated::SPI_ID,
            generated::SPI_FREQ,
            generated::SPI_SCK,
            generated::SPI_MOSI,
            generated::SPI_MISO,
        );
        let dc = RpOutputPin::new(generated::PIN_DC, false);
        let cs = RpOutputPin::new(generated::PIN_CS, true);
        let rst = RpOutputPin::new_optional(generated::PIN_RST, false);
        let bl = RpOutputPin::new(generated::PIN_BL, false);
        let delay = RpDelay::new();

        let mut display = St7789::new(
            spi,
            dc,
            cs,
            rst,
            bl,
            delay,
            generated::SCREEN_WIDTH,
            generated::SCREEN_HEIGHT,
            generated::MADCTL,
        );
        display.init();

        unsafe {
            addr_of_mut!(DISPLAY).write(Some(display));
        }
    }

    fn display() -> &'static mut Display {
        unsafe { (*addr_of_mut!(DISPLAY)).as_mut().unwrap() }
    }

    pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
        display().set_window(x0, y0, x1, y1);
    }

    pub fn write_pixels(data: &[u8]) {
        display().write_pixels(data);
    }

    pub fn set_backlight(on: bool) {
        display().set_backlight(on);
    }

    /// Composite low-power sequence: backlight off first (avoids a black flash
    /// while the panel is still powered), then DISPOFF, then SLPIN.
    pub fn display_sleep() {
        let d = display();
        d.set_backlight(false);
        d.display_off();
        d.sleep_in();
    }

    /// Composite wake sequence: SLPOUT (waits 120 ms internally), DISPON, then
    /// backlight on (avoids briefly showing the panel before its content is
    /// re-enabled).
    pub fn display_wake() {
        let d = display();
        d.sleep_out();
        d.display_on();
        d.set_backlight(true);
    }
}

#[cfg(not(has_display))]
mod inner {
    pub fn init() {}
    pub fn set_window(_x0: u16, _y0: u16, _x1: u16, _y1: u16) {}
    pub fn write_pixels(_data: &[u8]) {}
    pub fn set_backlight(_on: bool) {}
    pub fn display_sleep() {}
    pub fn display_wake() {}
}

pub use inner::*;

/// No-op on hardware — only meaningful for the sim's minifb window.
#[inline(always)]
pub fn update_window() {}

/// Always true on hardware — only meaningful for the sim's minifb window.
#[inline(always)]
pub fn is_window_open() -> bool {
    true
}
