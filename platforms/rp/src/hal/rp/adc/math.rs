// SPDX-License-Identifier: GPL-3.0-only
//! ADC scaling with no register in it — host-compilable, so the tests
//! actually run (`main.rs` pulls this file in by `#[path]` under
//! `cfg(test)`; the rest of `hal::rp` is ARM-only).

/// ADC reference voltage.
const VREF: f64 = 3.3;
/// Full-scale count of the 12-bit converter.
const ADC_MAX: f64 = 4095.0;

/// Convert a 12-bit raw conversion result to volts.
pub fn raw_to_volts(raw: u16) -> f64 {
    raw as f64 * VREF / ADC_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_raw_gives_zero_volts() {
        assert_eq!(raw_to_volts(0), 0.0);
    }

    #[test]
    fn max_raw_gives_vref() {
        assert!((raw_to_volts(4095) - 3.3).abs() < 1e-10);
    }

    #[test]
    fn midscale_raw_gives_half_vref() {
        // 2047 / 4095 * 3.3 ≈ 1.6496…
        let v = raw_to_volts(2047);
        assert!(v > 1.64 && v < 1.66);
    }
}
