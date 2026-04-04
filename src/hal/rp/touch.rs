//! XPT2046 resistive touch controller driver for the Waveshare 2.8" Pico display.
//!
//! Shares SPI1 with the ST7789 LCD. Uses a separate CS pin.
//!   GP16 — Touch CS
//!   GP17 — Touch IRQ (active low when touched)
//!   SPI1 MISO = GP12 (touch read data)

use super::gpio;
use super::spi;

const SPI_ID: u8 = 1;
const PIN_TOUCH_CS: u8 = 16;
const PIN_TOUCH_IRQ: u8 = 17;

// XPT2046 max SPI clock is ~2.5 MHz
const TOUCH_SPI_FREQ_HZ: u32 = 2_000_000;
// Restore display SPI frequency after touch read
const DISPLAY_SPI_FREQ_HZ: u32 = 62_500_000;

// XPT2046 control bytes
const CMD_READ_X: u8 = 0xD0; // X position, 12-bit, differential
const CMD_READ_Y: u8 = 0x90; // Y position, 12-bit, differential

// Calibration constants — raw ADC range mapped to screen coordinates.
// These are typical values; may need tuning per individual display.
const RAW_X_MIN: u16 = 200;
const RAW_X_MAX: u16 = 3900;
const RAW_Y_MIN: u16 = 200;
const RAW_Y_MAX: u16 = 3900;

/// Initialize the touch controller GPIO pins.
/// SPI1 must already be initialized by display::init().
pub fn init() {
    // Touch CS: output, initially high (deselected)
    gpio::set_direction(PIN_TOUCH_CS, 1);

    // Touch IRQ: input — configure pad for input enable.
    // We use direct PAC access since gpio::set_direction is output-only.
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Set GP17 to SIO function (5)
    p.IO_BANK0
        .gpio(PIN_TOUCH_IRQ as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(5) });

    // Configure pad: input enable, no pull (XPT2046 drives IRQ)
    p.PADS_BANK0.gpio(PIN_TOUCH_IRQ as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie()
            .set_bit()
            .od()
            .clear_bit()
            .pue()
            .clear_bit()
            .pde()
            .clear_bit()
    });

    // Disable output for this pin (input only)
    p.SIO
        .gpio_oe_clr()
        .write(|w| unsafe { w.bits(1u32 << PIN_TOUCH_IRQ) });

    // Also configure GP12 (MISO) for SPI1 function — needed for touch reads.
    // Display init only configures SCK (GP10) and MOSI (GP11).
    p.IO_BANK0
        .gpio(12)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(1) }); // 1 = SPI
    p.PADS_BANK0.gpio(12).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie().set_bit().od().clear_bit()
    });
}

/// Returns true if the touch panel is currently pressed (IRQ pin low).
pub fn is_pressed() -> bool {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    // Read GPIO input level
    (p.SIO.gpio_in().read().bits() & (1u32 << PIN_TOUCH_IRQ)) == 0
}

/// Read raw 12-bit X and Y ADC values from the XPT2046.
/// Returns None if touch is not active.
fn read_raw() -> Option<(u16, u16)> {
    if !is_pressed() {
        return None;
    }

    // Switch SPI1 to slower clock for touch controller
    spi::reconfigure(SPI_ID, TOUCH_SPI_FREQ_HZ, 0);

    gpio::set_value(PIN_TOUCH_CS, false);

    // Read X: send command byte, then read 2 bytes
    let tx_x = [CMD_READ_X, 0x00, 0x00];
    let mut rx_x = [0u8; 3];
    spi::transfer_raw(SPI_ID, &tx_x, &mut rx_x);
    let raw_x = ((rx_x[1] as u16) << 4) | ((rx_x[2] as u16) >> 4);

    // Read Y: send command byte, then read 2 bytes
    let tx_y = [CMD_READ_Y, 0x00, 0x00];
    let mut rx_y = [0u8; 3];
    spi::transfer_raw(SPI_ID, &tx_y, &mut rx_y);
    let raw_y = ((rx_y[1] as u16) << 4) | ((rx_y[2] as u16) >> 4);

    gpio::set_value(PIN_TOUCH_CS, true);

    // Restore display SPI frequency
    spi::reconfigure(SPI_ID, DISPLAY_SPI_FREQ_HZ, 0);

    Some((raw_x, raw_y))
}

/// Map a value from one range to another.
fn map_range(val: u16, in_min: u16, in_max: u16, out_min: u16, out_max: u16) -> u16 {
    let val = val.clamp(in_min, in_max);
    let in_range = (in_max - in_min) as u32;
    let out_range = (out_max - out_min) as u32;
    (out_min as u32 + (val - in_min) as u32 * out_range / in_range.max(1)) as u16
}

/// Read calibrated screen coordinates (0..319, 0..239).
/// Returns None if no touch is active.
pub fn read_point() -> Option<(u16, u16)> {
    let (raw_x, raw_y) = read_raw()?;
    let screen_x = map_range(raw_x, RAW_X_MIN, RAW_X_MAX, 0, super::display::WIDTH - 1);
    let screen_y = map_range(raw_y, RAW_Y_MIN, RAW_Y_MAX, 0, super::display::HEIGHT - 1);
    Some((screen_x, screen_y))
}
