// CLK_SYS defaults to system clock: 125 MHz on RP2040, 150 MHz on RP2350
#[cfg(feature = "chip-rp2040")]
const PCLK_HZ: u32 = 125_000_000;
#[cfg(feature = "chip-rp2350")]
const PCLK_HZ: u32 = 150_000_000;

// Compute (div_int, wrap) for a target PWM frequency.
//
// PWM freq ≈ PCLK_HZ / (div_int * (wrap + 1))
//
// Strategy: choose the highest possible wrap value (for duty-cycle resolution)
// while keeping div_int in [1, 255]. Fractional division (frac) is not used,
// keeping register writes simple.
fn clock_params(freq_hz: f64) -> (u8, u16) {
    // Integer approximation: truncate freq to u32, clamp to ≥ 1 Hz
    let freq_u32 = (freq_hz as u32).max(1);
    let period = PCLK_HZ / freq_u32;
    // Smallest div_int such that wrap = period / div_int fits in u16.
    let div_int = period.div_ceil(65536).clamp(1, 255) as u8;
    // Rounded division for wrap
    let wrap = ((period + div_int as u32 / 2) / div_int as u32).clamp(1, 65535) as u16;
    (div_int, wrap)
}

// Convert duty cycle percentage (0.0–100.0) to a compare register value.
// Uses u64 scaled integer arithmetic to avoid f64 methods not available in no_std
// and to prevent overflow when wrap is large (up to 65535).
fn duty_to_cc(duty_cycle: f64, wrap: u16) -> u16 {
    let scale: u64 = 1000;
    let duty_scaled = (duty_cycle * scale as f64) as u64; // e.g. 33.3% → 33300
    let top = wrap as u64 + 1;
    // Rounded: cc = (duty_scaled * top + scale/2) / (scale * 100)
    let cc = (duty_scaled * top + scale / 2) / (scale * 100);
    cc.min(top) as u16
}

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
    let (div_int, wrap) = clock_params(freq_hz);
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
pub(super) fn init(pin: u8) {
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
pub(super) fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool) {
    do_apply(pin, freq_hz, duty_cycle, enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_params_1khz_rp2040_div1() {
        // At 125 MHz, 1 kHz → period = 125_000
        // div_int = ceil(125000/65536) = 2, wrap = round(125000/2) = 62500
        let (div_int, wrap) = clock_params(1000.0);
        assert_eq!(div_int, 2);
        assert_eq!(wrap, 62500);
    }

    #[test]
    fn clock_params_50hz_fits_in_u16() {
        // 50 Hz → period = 2_500_000
        // div_int = ceil(2500000/65536) = 39, wrap = round(2500000/39) = 64103
        let (div_int, wrap) = clock_params(50.0);
        assert!(div_int >= 1 && div_int <= 255);
        assert!(wrap <= 65535);
    }

    #[test]
    fn clock_params_20khz_div1() {
        // 20 kHz → period = 6250
        // div_int = 1, wrap = 6250
        let (div_int, wrap) = clock_params(20_000.0);
        assert_eq!(div_int, 1);
        assert_eq!(wrap, 6250);
    }

    #[test]
    fn duty_to_cc_50_percent() {
        // wrap=9999, 50% → cc = round(0.5 * 10000) = 5000
        assert_eq!(duty_to_cc(50.0, 9999), 5000);
    }

    #[test]
    fn duty_to_cc_0_percent() {
        assert_eq!(duty_to_cc(0.0, 9999), 0);
    }

    #[test]
    fn duty_to_cc_100_percent() {
        // 100% → cc = wrap + 1 = 10000, clamped to 10000
        assert_eq!(duty_to_cc(100.0, 9999), 10000);
    }
}
