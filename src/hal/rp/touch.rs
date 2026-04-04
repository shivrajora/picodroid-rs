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

    // Configure pad: input enable, pull-up enabled.
    // XPT2046 PENIRQ is open-drain (active low) — needs a pull-up.
    p.PADS_BANK0.gpio(PIN_TOUCH_IRQ as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie()
            .set_bit()
            .od()
            .clear_bit()
            .pue()
            .set_bit()
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

    // Send an initial command to enable PENIRQ output.
    // After power-on the XPT2046 may not assert IRQ until it receives
    // a command with PD1=0, PD0=0 (power-down with PENIRQ enabled).
    spi::reconfigure(SPI_ID, TOUCH_SPI_FREQ_HZ, 0);
    gpio::set_value(PIN_TOUCH_CS, false);
    let tx = [CMD_READ_Y, 0x00, 0x00];
    let mut rx = [0u8; 3];
    spi::transfer_raw(SPI_ID, &tx, &mut rx);
    gpio::set_value(PIN_TOUCH_CS, true);
    spi::reconfigure(SPI_ID, DISPLAY_SPI_FREQ_HZ, 0);
}

/// Number of samples to take per read. The highest and lowest are
/// discarded and the rest averaged, which eliminates transient spikes
/// during finger lift/place.
const NUM_SAMPLES: usize = 5;

/// Read one raw 12-bit sample for a given command byte.
fn read_one(cmd: u8) -> u16 {
    let tx = [cmd, 0x00, 0x00];
    let mut rx = [0u8; 3];
    spi::transfer_raw(SPI_ID, &tx, &mut rx);
    ((rx[1] as u16) << 4) | ((rx[2] as u16) >> 4)
}

/// Read raw 12-bit X and Y values from the XPT2046 with multi-sample
/// averaging and outlier rejection. Returns None if no valid touch.
fn read_raw() -> Option<(u16, u16)> {
    spi::reconfigure(SPI_ID, TOUCH_SPI_FREQ_HZ, 0);
    gpio::set_value(PIN_TOUCH_CS, false);

    let mut xs = [0u16; NUM_SAMPLES];
    let mut ys = [0u16; NUM_SAMPLES];
    for i in 0..NUM_SAMPLES {
        xs[i] = read_one(CMD_READ_X);
        ys[i] = read_one(CMD_READ_Y);
    }

    gpio::set_value(PIN_TOUCH_CS, true);
    spi::reconfigure(SPI_ID, DISPLAY_SPI_FREQ_HZ, 0);

    // Sort and discard the lowest and highest sample (outlier rejection)
    xs.sort_unstable();
    ys.sort_unstable();
    let mid = &xs[1..NUM_SAMPLES - 1];
    let raw_x = (mid.iter().map(|&v| v as u32).sum::<u32>() / mid.len() as u32) as u16;
    let mid = &ys[1..NUM_SAMPLES - 1];
    let raw_y = (mid.iter().map(|&v| v as u32).sum::<u32>() / mid.len() as u32) as u16;

    // Reject if either axis is railed (no touch or noisy transition)
    if !(100..=4000).contains(&raw_x) || !(100..=4000).contains(&raw_y) {
        return None;
    }

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
