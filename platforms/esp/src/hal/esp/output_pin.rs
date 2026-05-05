// SPDX-License-Identifier: GPL-3.0-only
use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, OutputPin};

pub struct EspOutputPin {
    pin: u8,
}

impl EspOutputPin {
    pub fn new(pin: u8, _initially_high: bool) -> Self {
        Self { pin }
    }
}

impl ErrorType for EspOutputPin {
    type Error = Infallible;
}

impl OutputPin for EspOutputPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value(self.pin, false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value(self.pin, true);
        Ok(())
    }
}
