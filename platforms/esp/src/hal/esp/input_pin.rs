// SPDX-License-Identifier: GPL-3.0-only
use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, InputPin};

pub struct EspInputPin {
    _pin: u8,
}

impl EspInputPin {
    pub fn new(pin: u8, _pull_up: bool) -> Self {
        Self { _pin: pin }
    }
}

impl ErrorType for EspInputPin {
    type Error = Infallible;
}

impl InputPin for EspInputPin {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        Ok(true)
    }

    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(false)
    }
}
