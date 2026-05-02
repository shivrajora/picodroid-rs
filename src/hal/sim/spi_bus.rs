// SPDX-License-Identifier: GPL-3.0-only
//! Simulator `embedded-hal` SpiBus — delegates to the existing sim SPI stubs.

use core::convert::Infallible;
use embedded_hal::spi::{ErrorType, SpiBus};

use crate::drivers::SpiFreqSwitch;

pub struct SimSpiBus {
    spi_id: u8,
}

impl SimSpiBus {
    pub fn new_init(spi_id: u8, _freq_hz: u32) -> Self {
        super::spi::init(spi_id);
        Self { spi_id }
    }

    pub fn handle(spi_id: u8) -> Self {
        Self { spi_id }
    }
}

impl SpiFreqSwitch for SimSpiBus {
    fn set_frequency(&mut self, _freq_hz: u32) {
        super::spi::reconfigure(self.spi_id, _freq_hz, 0);
    }
}

impl ErrorType for SimSpiBus {
    type Error = Infallible;
}

impl SpiBus for SimSpiBus {
    fn read(&mut self, words: &mut [u8]) -> Result<(), Infallible> {
        let zeros = vec![0u8; words.len()];
        super::spi::transfer_raw(self.spi_id, &zeros, words);
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
        let tx = words.to_vec();
        super::spi::transfer_raw(self.spi_id, &tx, words);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
}
