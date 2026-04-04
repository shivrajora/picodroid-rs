//! `embedded-hal` OutputPin wrapper around the RP GPIO free functions.

use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, OutputPin};

pub struct RpOutputPin {
    pin: u8,
}

impl RpOutputPin {
    /// Create a new output pin. Configures the GPIO pad and sets the initial level.
    /// `initially_high`: true → start HIGH, false → start LOW.
    pub fn new(pin: u8, initially_high: bool) -> Self {
        super::gpio::set_direction(pin, if initially_high { 1 } else { 2 });
        Self { pin }
    }
}

impl ErrorType for RpOutputPin {
    type Error = Infallible;
}

impl OutputPin for RpOutputPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value(self.pin, false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        super::gpio::set_value(self.pin, true);
        Ok(())
    }
}
