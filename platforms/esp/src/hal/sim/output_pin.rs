// SPDX-License-Identifier: GPL-3.0-only
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
        super::gpio::set_value(self.pin, false);
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value(self.pin, true);
        Ok(())
    }
}
