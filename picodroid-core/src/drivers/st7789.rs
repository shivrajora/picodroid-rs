// SPDX-License-Identifier: GPL-3.0-only
//! Chip-agnostic ST7789 TFT display driver.
//!
//! Generic over `embedded-hal` traits — any MCU that provides
//! `SpiBus`, `OutputPin`, and `DelayNs` can use this driver.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiBus;

// ST7789 commands
const CMD_SWRESET: u8 = 0x01;
const CMD_SLPIN: u8 = 0x10;
const CMD_SLPOUT: u8 = 0x11;
const CMD_COLMOD: u8 = 0x3A;
const CMD_MADCTL: u8 = 0x36;
const CMD_INVON: u8 = 0x21;
const CMD_NORON: u8 = 0x13;
const CMD_DISPOFF: u8 = 0x28;
const CMD_DISPON: u8 = 0x29;
const CMD_CASET: u8 = 0x2A;
const CMD_RASET: u8 = 0x2B;
const CMD_RAMWR: u8 = 0x2C;

#[allow(dead_code)]
pub struct St7789<SPI, DC, CS, RST, BL, D> {
    spi: SPI,
    dc: DC,
    cs: CS,
    rst: RST,
    bl: BL,
    delay: D,
    width: u16,
    height: u16,
    madctl: u8,
}

impl<SPI, DC, CS, RST, BL, D> St7789<SPI, DC, CS, RST, BL, D>
where
    SPI: SpiBus,
    DC: OutputPin,
    CS: OutputPin,
    RST: OutputPin,
    BL: OutputPin,
    D: DelayNs,
{
    /// Create a new ST7789 driver. Does NOT initialize the display —
    /// call `init()` after construction.
    ///
    /// `madctl` sets the MADCTL register for display orientation/mirroring.
    /// Common values: 0x60 = landscape 320x240, 0x00 = portrait.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi: SPI,
        dc: DC,
        cs: CS,
        rst: RST,
        bl: BL,
        delay: D,
        width: u16,
        height: u16,
        madctl: u8,
    ) -> Self {
        Self {
            spi,
            dc,
            cs,
            rst,
            bl,
            delay,
            width,
            height,
            madctl,
        }
    }

    fn write_command(&mut self, cmd: u8) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_low(); // command mode
        let _ = self.spi.write(&[cmd]);
        let _ = self.cs.set_high();
    }

    #[allow(dead_code)]
    fn write_data(&mut self, data: &[u8]) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_high(); // data mode
        let _ = self.spi.write(data);
        let _ = self.cs.set_high();
    }

    fn write_command_data(&mut self, cmd: u8, data: &[u8]) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_low();
        let _ = self.spi.write(&[cmd]);
        let _ = self.dc.set_high();
        let _ = self.spi.write(data);
        let _ = self.cs.set_high();
    }

    /// Run the ST7789 initialization sequence (hardware reset + register config).
    pub fn init(&mut self) {
        // Hardware reset
        let _ = self.rst.set_low();
        self.delay.delay_ms(10);
        let _ = self.rst.set_high();
        self.delay.delay_ms(120);

        // Soft reset
        self.write_command(CMD_SWRESET);
        self.delay.delay_ms(150);

        // Sleep out
        self.write_command(CMD_SLPOUT);
        self.delay.delay_ms(50);

        // Color mode: 16-bit RGB565
        self.write_command_data(CMD_COLMOD, &[0x55]);

        // Memory data access control (orientation set by board config)
        self.write_command_data(CMD_MADCTL, &[self.madctl]);

        // Inversion on (ST7789 requires this for correct colors)
        self.write_command(CMD_INVON);

        // Normal display mode
        self.write_command(CMD_NORON);
        self.delay.delay_ms(10);

        // Display on
        self.write_command(CMD_DISPON);
        self.delay.delay_ms(10);
    }

    /// Set the active drawing window.
    pub fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        self.write_command_data(
            CMD_CASET,
            &[
                (x0 >> 8) as u8,
                (x0 & 0xFF) as u8,
                (x1 >> 8) as u8,
                (x1 & 0xFF) as u8,
            ],
        );
        self.write_command_data(
            CMD_RASET,
            &[
                (y0 >> 8) as u8,
                (y0 & 0xFF) as u8,
                (y1 >> 8) as u8,
                (y1 & 0xFF) as u8,
            ],
        );
        self.write_command(CMD_RAMWR);
    }

    /// Stream RGB565 pixel data to the display within the current window.
    pub fn write_pixels(&mut self, data: &[u8]) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_high(); // data mode
        let _ = self.spi.write(data);
        let _ = self.cs.set_high();
    }

    /// Turn the backlight on or off.
    pub fn set_backlight(&mut self, on: bool) {
        if on {
            let _ = self.bl.set_high();
        } else {
            let _ = self.bl.set_low();
        }
    }

    /// Enter low-power sleep mode. Datasheet requires ~5 ms before subsequent
    /// commands and ~120 ms before the next SLPOUT.
    pub fn sleep_in(&mut self) {
        self.write_command(CMD_SLPIN);
        self.delay.delay_ms(5);
    }

    /// Leave low-power sleep mode. Datasheet mandates a 120 ms wait before any
    /// further commands (matches the init sequence at startup).
    pub fn sleep_out(&mut self) {
        self.write_command(CMD_SLPOUT);
        self.delay.delay_ms(120);
    }

    /// Blank the display (panel RAM is retained).
    pub fn display_off(&mut self) {
        self.write_command(CMD_DISPOFF);
    }

    /// Show the display (after a prior `display_off` or fresh `sleep_out`).
    pub fn display_on(&mut self) {
        self.write_command(CMD_DISPON);
    }

    #[allow(dead_code)]
    pub fn width(&self) -> u16 {
        self.width
    }

    #[allow(dead_code)]
    pub fn height(&self) -> u16 {
        self.height
    }
}
