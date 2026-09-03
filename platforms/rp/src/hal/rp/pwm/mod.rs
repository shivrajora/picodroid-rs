// SPDX-License-Identifier: GPL-3.0-only
//! PWM: slice/channel routing and the register writes. The arithmetic is
//! in [`math`], where it can be tested on the host.

pub mod math;

use super::clock::PCLK_HZ;
use math::{clock_params, duty_to_cc};

// Disable slice → write DIV/TOP/CC → optionally re-enable.
//
// Note: DIV and TOP are shared between both channels (A and B) of a slice.
// The enable bit (CSR.EN) controls the whole slice. If both channels of the
// same slice are in use, reconfiguring one will affect the other's timing.
macro_rules! configure_ch {
    ($ch:expr, $channel:expr, $div_int:expr, $wrap:expr, $cc:expr, $enabled:expr) => {{
        // Disable before reconfiguring to avoid glitches
        $ch.csr().write(|w| w.en().clear_bit());
        // Clock divisor: integer only (frac=0 resets to 0 via write())
        $ch.div().write(|w| unsafe { w.int().bits($div_int) });
        // Counter wrap = period - 1
        $ch.top().write(|w| unsafe { w.top().bits($wrap) });
        // Compare value: modify to preserve the other channel's CC
        if $channel == 0 {
            $ch.cc().modify(|_, w| unsafe { w.a().bits($cc) });
        } else {
            $ch.cc().modify(|_, w| unsafe { w.b().bits($cc) });
        }
        // Re-enable if requested
        if $enabled {
            $ch.csr().write(|w| w.en().set_bit());
        }
    }};
}

fn do_apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    let slice = (pin / 2) % 8;
    let channel = pin % 2;
    let (div_int, wrap) = clock_params(PCLK_HZ, freq_hz);
    let cc = duty_to_cc(duty_cycle, wrap);

    match slice {
        0 => configure_ch!(p.PWM.ch(0), channel, div_int, wrap, cc, enabled),
        1 => configure_ch!(p.PWM.ch(1), channel, div_int, wrap, cc, enabled),
        2 => configure_ch!(p.PWM.ch(2), channel, div_int, wrap, cc, enabled),
        3 => configure_ch!(p.PWM.ch(3), channel, div_int, wrap, cc, enabled),
        4 => configure_ch!(p.PWM.ch(4), channel, div_int, wrap, cc, enabled),
        5 => configure_ch!(p.PWM.ch(5), channel, div_int, wrap, cc, enabled),
        6 => configure_ch!(p.PWM.ch(6), channel, div_int, wrap, cc, enabled),
        _ => configure_ch!(p.PWM.ch(7), channel, div_int, wrap, cc, enabled),
    }
}

/// Configure GPIO pin for PWM function and apply default settings (1 kHz, 0% duty, disabled).
pub fn init(pin: u8) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Ensure IO_BANK0 and PADS_BANK0 are out of reset (idempotent)
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Release PWM block from reset (idempotent — safe to call for each pin)
    p.RESETS.reset().modify(|_, w| w.pwm().clear_bit());
    while p.RESETS.reset_done().read().pwm().bit_is_clear() {}

    // Route GPIO pin to PWM function (funcsel = 4 on both RP2040 and RP2350)
    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(4) });
    p.PADS_BANK0.gpio(pin as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie().set_bit().od().clear_bit()
    });

    // Apply defaults: 1 kHz, 0% duty cycle, disabled
    do_apply(pin, 1000.0, 0.0, false);
}

/// Apply full PWM configuration — used by setEnabled, setPwmFrequencyHz, setPwmDutyCycle.
pub fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool) {
    do_apply(pin, freq_hz, duty_cycle, enabled);
}
