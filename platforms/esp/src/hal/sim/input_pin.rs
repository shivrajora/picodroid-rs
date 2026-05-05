// SPDX-License-Identifier: GPL-3.0-only
use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, InputPin};

pub struct SimInputPin;

impl SimInputPin {
    pub fn new(_pin: u8, _pull_up: bool) -> Self {
        Self
    }
}

impl ErrorType for SimInputPin {
    type Error = Infallible;
}

impl InputPin for SimInputPin {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        Ok(true)
    }
    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(false)
    }
}
