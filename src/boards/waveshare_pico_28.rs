//! Board configuration for the Waveshare 2.8" Pico display (ST7789 + XPT2046).
//!
//! Pin mapping (directly on RP2350 Pico 2):
//!   SPI1: GP10 (SCK), GP11 (MOSI)
//!   GP8  — LCD DC (Data/Command select)
//!   GP9  — LCD CS
//!   GP12 — Touch MISO (SPI1 RX)
//!   GP13 — LCD BL (Backlight)
//!   GP15 — LCD RST
//!   GP16 — Touch CS
//!   GP17 — Touch IRQ (open-drain, pull-up)

use crate::drivers::st7789::St7789;
use crate::drivers::xpt2046::Xpt2046;
use crate::hal::delay::RpDelay;
use crate::hal::input_pin::RpInputPin;
use crate::hal::output_pin::RpOutputPin;
use crate::hal::spi_bus::RpSpiBus;

// --- Display constants ---
pub const SCREEN_WIDTH: u16 = 320;
pub const SCREEN_HEIGHT: u16 = 240;

const SPI_ID: u8 = 1;
const DISPLAY_SPI_FREQ: u32 = 62_500_000;

// Display pins
const PIN_DC: u8 = 8;
const PIN_CS: u8 = 9;
const PIN_RST: u8 = 15;
const PIN_BL: u8 = 13;

// --- Touch constants ---
const TOUCH_SPI_FREQ: u32 = 2_000_000;
const PIN_TOUCH_CS: u8 = 16;
const PIN_TOUCH_IRQ: u8 = 17;
const PIN_TOUCH_MISO: u8 = 12;

// Calibration: raw ADC range mapped to screen coordinates
const CAL_X_MIN: u16 = 200;
const CAL_X_MAX: u16 = 3900;
const CAL_Y_MIN: u16 = 200;
const CAL_Y_MAX: u16 = 3900;

// --- Concrete types for this board ---
pub type Display = St7789<RpSpiBus, RpOutputPin, RpOutputPin, RpOutputPin, RpOutputPin, RpDelay>;
pub type Touch = Xpt2046<RpSpiBus, RpOutputPin>;

/// Configure GP12 (MISO) for SPI1 function — needed for touch reads.
/// Display init only configures SCK (GP10) and MOSI (GP11).
fn configure_touch_miso() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    p.IO_BANK0
        .gpio(PIN_TOUCH_MISO as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(1) }); // 1 = SPI
    p.PADS_BANK0.gpio(PIN_TOUCH_MISO as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie().set_bit().od().clear_bit()
    });
}

/// Create and initialize the ST7789 display driver for this board.
pub fn create_display() -> Display {
    // Initialize SPI1 first — spi::init() configures GP8 as SPI MISO,
    // but we need GP8 as GPIO for DC. The RpOutputPin::new() below
    // overrides GP8 to SIO function.
    let spi = RpSpiBus::new_init(SPI_ID, DISPLAY_SPI_FREQ);
    let dc = RpOutputPin::new(PIN_DC, false);
    let cs = RpOutputPin::new(PIN_CS, true); // CS starts high (deselected)
    let rst = RpOutputPin::new(PIN_RST, false);
    let bl = RpOutputPin::new(PIN_BL, false); // backlight off initially
    let delay = RpDelay::new();

    let mut display = St7789::new(spi, dc, cs, rst, bl, delay, SCREEN_WIDTH, SCREEN_HEIGHT);
    display.init();
    display
}

/// Create and initialize the XPT2046 touch driver for this board.
pub fn create_touch() -> Touch {
    // Configure MISO pin and IRQ input for touch
    configure_touch_miso();
    let _irq = RpInputPin::new(PIN_TOUCH_IRQ, true); // pull-up for open-drain PENIRQ

    // Use a separate SPI handle (same physical bus, no re-init)
    let spi = RpSpiBus::handle(SPI_ID);
    let cs = RpOutputPin::new(PIN_TOUCH_CS, true); // CS starts high

    let mut touch = Xpt2046::new(
        spi,
        cs,
        TOUCH_SPI_FREQ,
        DISPLAY_SPI_FREQ,
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        CAL_X_MIN,
        CAL_X_MAX,
        CAL_Y_MIN,
        CAL_Y_MAX,
    );
    touch.init();
    touch
}
