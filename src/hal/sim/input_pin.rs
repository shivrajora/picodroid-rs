//! Simulator `embedded-hal` InputPin — always reads as not pressed.

use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, InputPin};

pub struct SimInputPin {
    _pin: u8,
}

impl SimInputPin {
    pub fn new(pin: u8, _pull_up: bool) -> Self {
        println!("[sim] GP{pin}: input (pull_up={_pull_up})");
        Self { _pin: pin }
    }
}

impl ErrorType for SimInputPin {
    type Error = Infallible;
}

impl InputPin for SimInputPin {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        Ok(true) // no touch in sim
    }

    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(false)
    }
}
