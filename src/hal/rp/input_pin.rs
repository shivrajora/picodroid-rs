//! `embedded-hal` InputPin wrapper with RP PAC register setup.
//!
//! Handles input-enable, pull-up, function select, and RP2350 ISO bit —
//! all the configuration that `gpio.rs` (output-only) doesn't cover.

use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, InputPin};

pub struct RpInputPin {
    pin: u8,
}

impl RpInputPin {
    /// Configure a GPIO pin as input.
    /// `pull_up`: true → enable internal pull-up (needed for open-drain signals like XPT2046 PENIRQ).
    pub fn new(pin: u8, pull_up: bool) -> Self {
        #[cfg(feature = "chip-rp2350-hal")]
        use rp235x_hal::pac;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::pac;
        let p = unsafe { pac::Peripherals::steal() };

        // Ensure IO_BANK0 and PADS_BANK0 are out of reset
        p.RESETS
            .reset()
            .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
        while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
        while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

        // Set GPIO function to SIO (5)
        p.IO_BANK0
            .gpio(pin as usize)
            .gpio_ctrl()
            .write(|w| unsafe { w.funcsel().bits(5) });

        // Configure pad: input enable, optional pull-up
        p.PADS_BANK0.gpio(pin as usize).write(|w| {
            #[cfg(feature = "chip-rp2350-hal")]
            let w = w.iso().clear_bit();
            let w = w.ie().set_bit().od().clear_bit();
            if pull_up {
                w.pue().set_bit().pde().clear_bit()
            } else {
                w.pue().clear_bit().pde().clear_bit()
            }
        });

        // Disable output driver (input only)
        p.SIO
            .gpio_oe_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });

        Self { pin }
    }
}

impl ErrorType for RpInputPin {
    type Error = Infallible;
}

impl InputPin for RpInputPin {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        #[cfg(feature = "chip-rp2350-hal")]
        use rp235x_hal::pac;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::pac;
        let p = unsafe { pac::Peripherals::steal() };
        let val = p.SIO.gpio_in().read().bits();
        Ok((val >> self.pin) & 1 != 0)
    }

    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(!self.is_high().unwrap())
    }
}
