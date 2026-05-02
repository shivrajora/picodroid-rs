// SPDX-License-Identifier: GPL-3.0-only
// ADC reference voltage and resolution
const VREF: f64 = 3.3;
const ADC_MAX: f64 = 4095.0; // 12-bit ADC

/// Configure a GPIO pin (26–29) for ADC analog input and enable the ADC peripheral.
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

    // Release ADC from reset (idempotent — safe to call for each pin)
    p.RESETS.reset().modify(|_, w| w.adc().clear_bit());
    while p.RESETS.reset_done().read().adc().bit_is_clear() {}

    // Enable ADC peripheral
    p.ADC.cs().write(|w| w.en().set_bit());

    // Set GPIO function to NULL (0x1f) — disconnects pin from digital logic
    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(0x1f) });

    // Disable all pad features for analog: no input enable, no output disable override,
    // no pull-up, no pull-down, no schmitt trigger (all cleared by write())
    p.PADS_BANK0.gpio(pin as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie().clear_bit().od().clear_bit()
    });
}

/// Perform a single ADC conversion on the given GPIO pin (26–29) and return voltage in volts.
pub fn read(pin: u8) -> f64 {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    let channel = pin - 26; // GP26 → channel 0, GP27 → 1, GP28 → 2, GP29 → 3

    // Select ADC channel and trigger a single conversion
    p.ADC
        .cs()
        .modify(|_, w| unsafe { w.ainsel().bits(channel).start_once().set_bit() });

    // Poll READY bit (busy-wait for conversion to complete, typically ~2 µs)
    while p.ADC.cs().read().ready().bit_is_clear() {}

    // Convert 12-bit raw result to voltage
    let raw = p.ADC.result().read().result().bits() as f64;
    raw * VREF / ADC_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adc_min_raw_gives_zero_volts() {
        let voltage = 0.0_f64 * VREF / ADC_MAX;
        assert_eq!(voltage, 0.0);
    }

    #[test]
    fn adc_max_raw_gives_vref() {
        let voltage = ADC_MAX * VREF / ADC_MAX;
        // Should equal VREF (3.3 V) within floating point precision
        assert!((voltage - 3.3).abs() < 1e-10);
    }

    #[test]
    fn adc_midscale_raw_gives_half_vref() {
        // 2047 / 4095 * 3.3 ≈ 1.6496…
        let voltage = 2047.0_f64 * VREF / ADC_MAX;
        assert!(voltage > 1.64 && voltage < 1.66);
    }
}
