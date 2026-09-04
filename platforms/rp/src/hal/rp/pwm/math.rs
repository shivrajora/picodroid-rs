// SPDX-License-Identifier: GPL-3.0-only
//! PWM register arithmetic with no register in it — host-compilable, so the
//! tests actually run (`main.rs` pulls this file in by `#[path]` under
//! `cfg(test)`; the rest of `hal::rp` is ARM-only).

/// Compute `(div_int, wrap)` for a target PWM frequency on a slice clocked
/// at `pclk_hz`.
///
/// PWM freq ≈ pclk_hz / (div_int * (wrap + 1))
///
/// Strategy: choose the highest possible wrap value (for duty-cycle
/// resolution) while keeping div_int in [1, 255]. Fractional division (frac)
/// is not used, keeping register writes simple.
pub fn clock_params(pclk_hz: u32, freq_hz: f64) -> (u8, u16) {
    // Integer approximation: truncate freq to u32, clamp to ≥ 1 Hz
    let freq_u32 = (freq_hz as u32).max(1);
    let period = pclk_hz / freq_u32;
    // Smallest div_int such that wrap = period / div_int fits in u16.
    let div_int = period.div_ceil(65536).clamp(1, 255) as u8;
    // Rounded division for wrap
    let wrap = ((period + div_int as u32 / 2) / div_int as u32).clamp(1, 65535) as u16;
    (div_int, wrap)
}

/// Convert a duty cycle percentage (0.0–100.0) to a compare register value.
///
/// Uses u64 scaled integer arithmetic to avoid f64 methods not available in
/// no_std and to prevent overflow when wrap is large (up to 65535).
pub fn duty_to_cc(duty_cycle: f64, wrap: u16) -> u16 {
    let scale: u64 = 1000;
    let duty_scaled = (duty_cycle * scale as f64) as u64; // e.g. 33.3% → 33300
    let top = wrap as u64 + 1;
    // Rounded: cc = (duty_scaled * top + scale/2) / (scale * 100)
    let cc = (duty_scaled * top + scale / 2) / (scale * 100);
    cc.min(top) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both chips' peripheral clocks, so the math is checked at each rather
    // than at whichever board the test build happens to select.
    const RP2040_PCLK: u32 = 125_000_000;
    const RP2350_PCLK: u32 = 150_000_000;

    #[test]
    fn clock_params_1khz_at_125mhz_needs_div2() {
        // period = 125_000; div_int = ceil(125000/65536) = 2,
        // wrap = round(125000/2) = 62500
        assert_eq!(clock_params(RP2040_PCLK, 1000.0), (2, 62500));
    }

    #[test]
    fn clock_params_1khz_at_150mhz_needs_div3() {
        // period = 150_000; div_int = ceil(150000/65536) = 3,
        // wrap = round(150000/3) = 50000
        assert_eq!(clock_params(RP2350_PCLK, 1000.0), (3, 50000));
    }

    #[test]
    fn clock_params_50hz_fits_in_u16() {
        for pclk in [RP2040_PCLK, RP2350_PCLK] {
            let (div_int, wrap) = clock_params(pclk, 50.0);
            assert!((1..=255).contains(&div_int));
            assert!(wrap <= 65535);
        }
    }

    #[test]
    fn clock_params_20khz_div1() {
        // period = 6250 at 125 MHz: div_int = 1, wrap = 6250
        assert_eq!(clock_params(RP2040_PCLK, 20_000.0), (1, 6250));
    }

    #[test]
    fn clock_params_never_divides_by_zero_hz() {
        // 0 Hz is clamped to 1 Hz rather than dividing by zero.
        let (div_int, wrap) = clock_params(RP2040_PCLK, 0.0);
        assert!((1..=255).contains(&div_int));
        assert!(wrap >= 1);
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
