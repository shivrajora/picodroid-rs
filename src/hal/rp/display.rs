//! ST7789 TFT display driver for the Waveshare 2.8" Pico display.
//!
//! Pin mapping (directly on RP2350 Pico 2):
//!   SPI1: GP10 (SCK), GP11 (MOSI)
//!   GP8  — LCD DC (Data/Command select)
//!   GP9  — LCD CS (Chip Select)
//!   GP15 — LCD RST (Reset)
//!   GP13 — LCD BL (Backlight)

use super::gpio;
use super::spi;

const SPI_ID: u8 = 1;
const PIN_DC: u8 = 8;
const PIN_CS: u8 = 9;
const PIN_RST: u8 = 15;
const PIN_BL: u8 = 13;

pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 240;

// SPI clock for display: 62.5 MHz (ST7789 max is ~62.5 MHz at 1.8V, safe at 3.3V)
const SPI_FREQ_HZ: u32 = 62_500_000;

// ST7789 commands
const CMD_SWRESET: u8 = 0x01;
const CMD_SLPOUT: u8 = 0x11;
const CMD_COLMOD: u8 = 0x3A;
const CMD_MADCTL: u8 = 0x36;
const CMD_INVON: u8 = 0x21;
const CMD_NORON: u8 = 0x13;
const CMD_DISPON: u8 = 0x29;
const CMD_CASET: u8 = 0x2A;
const CMD_RASET: u8 = 0x2B;
const CMD_RAMWR: u8 = 0x2C;

fn delay_ms(ms: u32) {
    // Use FreeRTOS delay if scheduler is running, otherwise busy-wait.
    // During early init (before scheduler) we use a rough busy loop.
    cortex_m::asm::delay(ms * 150_000); // ~1ms per iteration at 150 MHz
}

fn write_command(cmd: u8) {
    gpio::set_value(PIN_CS, false);
    gpio::set_value(PIN_DC, false); // command mode
    spi::write_raw(SPI_ID, &[cmd]);
    gpio::set_value(PIN_CS, true);
}

fn write_data(data: &[u8]) {
    gpio::set_value(PIN_CS, false);
    gpio::set_value(PIN_DC, true); // data mode
    spi::write_raw(SPI_ID, data);
    gpio::set_value(PIN_CS, true);
}

fn write_command_data(cmd: u8, data: &[u8]) {
    gpio::set_value(PIN_CS, false);
    gpio::set_value(PIN_DC, false);
    spi::write_raw(SPI_ID, &[cmd]);
    gpio::set_value(PIN_DC, true);
    spi::write_raw(SPI_ID, data);
    gpio::set_value(PIN_CS, true);
}

/// Initialize the ST7789 display.
pub fn init() {
    // Configure control pins as GPIO outputs
    // direction=2 means OUT_INITIALLY_LOW
    gpio::set_direction(PIN_DC, 2);
    gpio::set_direction(PIN_CS, 1); // CS starts high (deselected)
    gpio::set_direction(PIN_RST, 2);
    gpio::set_direction(PIN_BL, 2); // backlight off initially

    // Initialize SPI1 (SCK=GP10, MOSI=GP11 already configured by spi::init)
    spi::init(SPI_ID);
    spi::reconfigure(SPI_ID, SPI_FREQ_HZ, 0); // MODE_0

    // Hardware reset
    gpio::set_value(PIN_RST, false);
    delay_ms(10);
    gpio::set_value(PIN_RST, true);
    delay_ms(120);

    // ST7789 initialization sequence
    write_command(CMD_SWRESET);
    delay_ms(150);

    write_command(CMD_SLPOUT);
    delay_ms(50);

    // Color mode: 16-bit RGB565
    write_command_data(CMD_COLMOD, &[0x55]);

    // Memory data access control: landscape orientation
    // MY=0, MX=1, MV=1 → 320x240 landscape; BGR order
    write_command_data(CMD_MADCTL, &[0x60 | 0x08]);

    // Inversion on (ST7789 requires this for correct colors)
    write_command(CMD_INVON);

    // Normal display mode
    write_command(CMD_NORON);
    delay_ms(10);

    // Display on
    write_command(CMD_DISPON);
    delay_ms(10);
}

/// Set the active drawing window.
pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
    write_command_data(
        CMD_CASET,
        &[
            (x0 >> 8) as u8,
            (x0 & 0xFF) as u8,
            (x1 >> 8) as u8,
            (x1 & 0xFF) as u8,
        ],
    );
    write_command_data(
        CMD_RASET,
        &[
            (y0 >> 8) as u8,
            (y0 & 0xFF) as u8,
            (y1 >> 8) as u8,
            (y1 & 0xFF) as u8,
        ],
    );
    write_command(CMD_RAMWR);
}

/// Stream RGB565 pixel data to the display within the current window.
pub fn write_pixels(data: &[u8]) {
    gpio::set_value(PIN_CS, false);
    gpio::set_value(PIN_DC, true); // data mode
    spi::write_raw(SPI_ID, data);
    gpio::set_value(PIN_CS, true);
}

/// Turn the backlight on or off.
pub fn set_backlight(on: bool) {
    gpio::set_value(PIN_BL, on);
}
