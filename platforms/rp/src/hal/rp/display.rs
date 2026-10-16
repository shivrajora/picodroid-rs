// SPDX-License-Identifier: GPL-3.0-only
//! Display facade — delegates to the generic ST7789 driver via board config.
//!
//! Display constants and pin mappings come from the build.rs-generated
//! `display_config.rs`, driven by the `[display]` section in board.toml.

// board.toml lists every pin/geometry field the panel has; a given build
// consumes only the subset its code path touches. Scoped here rather than
// over the whole HAL module so real rot outside the generated table stays
// visible.
#[allow(dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
}

pub const WIDTH: u16 = generated::SCREEN_WIDTH;
pub const HEIGHT: u16 = generated::SCREEN_HEIGHT;
pub const BAND_HEIGHT: usize = generated::BAND_HEIGHT;
pub const SCROLL_LIMIT: u8 = generated::SCROLL_LIMIT;
pub const DRAW_BUFFERS: usize = generated::DRAW_BUFFERS;

#[cfg(has_display)]
mod inner {
    use super::generated;
    use crate::hal::delay::RpDelay;
    use crate::hal::output_pin::RpOutputPin;
    use crate::hal::spi_bus::RpSpiBus;
    use freertos_rust::{Duration, Mutex};
    use picodroid_core::util::set_once::SetOnce;

    // The panel controller board.toml names. Both drivers expose the same
    // constructor and the same method set, so the facade below is written once
    // against `Panel` and only the `use` changes per board. Exactly one
    // `display_*` cfg is ever set — build.rs checks the name against
    // `KNOWN_DISPLAY_DRIVERS` — so these arms are mutually exclusive.
    #[cfg(display_st7789)]
    use crate::drivers::st7789::St7789 as Panel;
    #[cfg(display_st7796)]
    use crate::drivers::st7796::St7796 as Panel;

    type Display = Panel<RpSpiBus, RpOutputPin, RpOutputPin, RpOutputPin, RpOutputPin, RpDelay>;

    // The panel's `spi_freq` must be a rate the controller can program
    // exactly. `clock_divisors` floors, so an inexact request is rounded *up*
    // (62.5 MHz at a 150 MHz PCLK became 75 MHz, unasked, on every RP2350
    // board until 2026-10); a panel's rated write clock is a ceiling, so the
    // board has to ask for the rate it will get. The achievable rates are
    // PCLK / 2, PCLK / 4, PCLK / 6, …
    const _: () = assert!(
        crate::hal::spi::xfer::is_exact_hz(crate::hal::chip::clock::PCLK_HZ, generated::SPI_FREQ),
        "[display] spi_freq is not a rate this chip's SPI controller can program exactly \
         (PCLK / 2, PCLK / 4, PCLK / 6, ...); it would be rounded up past the request"
    );

    /// The panel driver. Stored once by `init` at board bring-up, before
    /// LVGL runs; reached afterwards through [`with_display`], one caller
    /// at a time. Today every caller is the UI task — its flush and
    /// sleep/wake paths make every call below, and on a board where the
    /// touch controller shares the display's bus, its touch read collects
    /// the band first — but that is a fact about the callers, and the
    /// mutex is what the type promises (bugs-unsafe-review-2026-10.md,
    /// UR-5). Uncontended, a take and a give per call.
    static DISPLAY: SetOnce<Mutex<Display>> = SetOnce::new();

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
        // Reset and backlight are both optional: a module may tie either line
        // high in hardware (the Enviro+ Pack has no reset, the 52Pi EP-0172 no
        // backlight). `new_optional` makes the pin's writes no-ops on `None`.
        let rst = RpOutputPin::new_optional(generated::PIN_RST, false);
        let bl = RpOutputPin::new_optional(generated::PIN_BL, false);
        let delay = RpDelay::new();

        let mut display = Panel::new(
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

        let display = Mutex::new(display).expect("display mutex");
        // SAFETY: board bring-up, before LVGL — and so the UI task's first
        // call into this module — exists; nothing else reads the cell.
        unsafe { DISPLAY.set(display) };
    }

    /// The driver, for the duration of `f`. Blocks while another caller
    /// has it. Not re-entrant: `f` must not reach this module again (no
    /// panel method does).
    fn with_display<R>(f: impl FnOnce(&mut Display) -> R) -> R {
        let mut d = DISPLAY
            .get()
            .expect("display not initialised")
            .lock(Duration::infinite())
            .expect("display mutex take");
        f(&mut d)
    }

    pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
        with_display(|d| d.set_window(x0, y0, x1, y1));
    }

    pub fn write_pixels(data: &[u8]) {
        with_display(|d| d.write_pixels(data));
    }

    /// The asynchronous half of the flush: the band goes out by DMA and the
    /// driver returns at once, holding chip select and the bus lock until
    /// `write_pixels_wait` — or its next command — collects the completion.
    pub fn write_pixels_start(data: &[u8]) {
        with_display(|d| d.write_pixels_start(data));
    }

    pub fn write_pixels_wait() {
        with_display(|d| d.write_pixels_wait());
    }

    pub fn set_backlight(on: bool) {
        with_display(|d| d.set_backlight(on));
    }

    /// The panel's own vertical scroll (VSCRDEF / VSCRSADD). Reached only on
    /// a board where `board_cfg::hw_vscroll` holds — a portrait ST7796 —
    /// though both drivers carry the two commands.
    pub fn set_vertical_scroll_area(top_fixed: u16, rows: u16) {
        with_display(|d| d.set_vertical_scroll_area(top_fixed, rows));
    }

    pub fn set_vertical_scroll_start(line: u16) {
        with_display(|d| d.set_vertical_scroll_start(line));
    }

    /// Composite low-power sequence: backlight off first (avoids a black flash
    /// while the panel is still powered), then DISPOFF, then SLPIN.
    pub fn display_sleep() {
        with_display(|d| {
            d.set_backlight(false);
            d.display_off();
            d.sleep_in();
        });
    }

    /// Composite wake sequence: SLPOUT (waits 120 ms internally), DISPON, then
    /// backlight on (avoids briefly showing the panel before its content is
    /// re-enabled).
    pub fn display_wake() {
        with_display(|d| {
            d.sleep_out();
            d.display_on();
            d.set_backlight(true);
        });
    }
}

#[cfg(not(has_display))]
mod inner {
    pub fn init() {}
    pub fn set_window(_x0: u16, _y0: u16, _x1: u16, _y1: u16) {}
    pub fn write_pixels(_data: &[u8]) {}
    pub fn write_pixels_start(_data: &[u8]) {}
    pub fn write_pixels_wait() {}
    pub fn set_backlight(_on: bool) {}
    pub fn set_vertical_scroll_area(_top_fixed: u16, _rows: u16) {}
    pub fn set_vertical_scroll_start(_line: u16) {}
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
