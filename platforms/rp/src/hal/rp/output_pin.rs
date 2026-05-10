// SPDX-License-Identifier: GPL-3.0-only
//! `embedded-hal` OutputPin wrapper around the RP GPIO free functions.

use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, OutputPin};

pub struct RpOutputPin {
    pin: Option<u8>,
}

impl RpOutputPin {
    /// Create a new output pin. Configures the GPIO pad and sets the initial level.
    /// `initially_high`: true → start HIGH, false → start LOW.
    pub fn new(pin: u8, initially_high: bool) -> Self {
        Self::new_optional(Some(pin), initially_high)
    }

    /// Create an output pin from an optional pin number. When `pin` is `None`,
    /// `set_low`/`set_high` no-op — used for boards that tie a control line
    /// (e.g. ST7789 RST) high in hardware.
    pub fn new_optional(pin: Option<u8>, initially_high: bool) -> Self {
        if let Some(p) = pin {
            super::gpio::set_direction(p, if initially_high { 1 } else { 2 });
        }
        Self { pin }
    }
}

impl ErrorType for RpOutputPin {
    type Error = Infallible;
}

impl OutputPin for RpOutputPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        if let Some(p) = self.pin {
            super::gpio::set_value(p, false);
        }
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        if let Some(p) = self.pin {
            super::gpio::set_value(p, true);
        }
        Ok(())
    }
}
