// SPDX-License-Identifier: GPL-3.0-only
//! Simulator `embedded-hal` OutputPin.
//!
//! Driver-level pin toggles (e.g. SPI CS bit-bang in `Xpt2046::sample`) fire
//! at LVGL tick rate, so they bypass the per-toggle println in `gpio.rs` and
//! write the GPIO_OUT atomic directly. The one-shot direction print at
//! `new()` is still useful and stays.

use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, OutputPin};

pub struct SimOutputPin {
    pin: u8,
}

impl SimOutputPin {
    pub fn new(pin: u8, initially_high: bool) -> Self {
        super::gpio::set_direction(pin, if initially_high { 1 } else { 2 });
        Self { pin }
    }
}

impl ErrorType for SimOutputPin {
    type Error = Infallible;
}

impl OutputPin for SimOutputPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value_silent(self.pin, false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value_silent(self.pin, true);
        Ok(())
    }
}
