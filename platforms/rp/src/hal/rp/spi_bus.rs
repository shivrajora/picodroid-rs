// SPDX-License-Identifier: GPL-3.0-only
//! `embedded-hal` SpiBus wrapper around the RP SPI free functions.
//!
//! Multiple `RpSpiBus` handles with the same `spi_id` are safe to create:
//! the underlying SPI peripheral is global state accessed via `Peripherals::steal()`,
//! and access is non-concurrent in single-task FreeRTOS contexts.

use core::convert::Infallible;
use embedded_hal::spi::{ErrorType, SpiBus};

use crate::drivers::SpiFreqSwitch;

pub struct RpSpiBus {
    spi_id: u8,
}

impl RpSpiBus {
    /// Initialize the SPI peripheral and configure it at `freq_hz`, MODE_0.
    /// Uses chip-default pins for the bus.
    pub fn new_init(spi_id: u8, freq_hz: u32) -> Self {
        Self::new_init_with_pins(spi_id, freq_hz, None, None, None)
    }

    /// Initialize the SPI peripheral on the given pad set. `None` for any pin
    /// falls back to the chip default for that bus. Used by display init when
    /// the board.toml specifies non-default SPI pads (e.g. Pimoroni Pico
    /// Enviro+ Pack with SPI0 SCK=GP18, MOSI=GP19).
    pub fn new_init_with_pins(
        spi_id: u8,
        freq_hz: u32,
        sck: Option<u8>,
        mosi: Option<u8>,
        miso: Option<u8>,
    ) -> Self {
        super::spi::init_with_pins(spi_id, sck, mosi, miso);
        super::spi::reconfigure(spi_id, freq_hz, 0);
        Self { spi_id }
    }

    /// Create a handle to an already-initialized SPI bus (no hardware init).
    pub fn handle(spi_id: u8) -> Self {
        Self { spi_id }
    }
}

impl SpiFreqSwitch for RpSpiBus {
    fn set_frequency(&mut self, freq_hz: u32) {
        super::spi::reconfigure(self.spi_id, freq_hz, 0);
    }
}

impl ErrorType for RpSpiBus {
    type Error = Infallible;
}

impl SpiBus for RpSpiBus {
    fn read(&mut self, words: &mut [u8]) -> Result<(), Infallible> {
        // Send zeros, collect received bytes — use a stack buffer to avoid alloc.
        let mut pos = 0;
        while pos < words.len() {
            let chunk = (words.len() - pos).min(32);
            let zeros = [0u8; 32];
            super::spi::transfer_raw(self.spi_id, &zeros[..chunk], &mut words[pos..pos + chunk]);
            pos += chunk;
        }
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Infallible> {
        super::spi::write_raw(self.spi_id, words);
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Infallible> {
        super::spi::transfer_raw(self.spi_id, write, read);
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Infallible> {
        // Copy to stack buffer in chunks, then transfer.
        let mut pos = 0;
        while pos < words.len() {
            let chunk = (words.len() - pos).min(32);
            let mut tx = [0u8; 32];
            tx[..chunk].copy_from_slice(&words[pos..pos + chunk]);
            super::spi::transfer_raw(self.spi_id, &tx[..chunk], &mut words[pos..pos + chunk]);
            pos += chunk;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
}
