// SPDX-License-Identifier: GPL-3.0-only
//! UART baud-rate arithmetic with no register in it — host-compilable, so the
//! tests run (`main.rs` pulls this file in by `#[path]` under `cfg(test)`,
//! as for the PWM and I2C math).

/// PL011 `UARTIBRD` / `UARTFBRD` for `baudrate` on a peripheral clock of
/// `pclk_hz`:
///
/// ```text
/// BRD = PCLK / (16 * baud)
/// brd_x64 = (PCLK * 4) / baud   (ibrd in the upper bits, fbrd in the low 6)
/// ```
///
/// The rate is clamped to what the divider can express: a zero or negative
/// rate (an app's `setBaudRate(0)`) used to divide by zero and reset the
/// board, and a rate above `PCLK / 16` yields IBRD 0, which the PL011
/// documents as invalid, so the integer part is floored at 1.
pub fn baud_divisors(pclk_hz: u32, baudrate: i32) -> (u16, u8) {
    let baud = baudrate.max(1) as u64;
    let brd_x64 = (pclk_hz as u64 * 4) / baud;
    let ibrd = ((brd_x64 >> 6).min(u16::MAX as u64) as u16).max(1);
    let fbrd = (brd_x64 & 0x3F) as u8;
    (ibrd, fbrd)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RP2040_PCLK: u32 = 125_000_000;
    const RP2350_PCLK: u32 = 150_000_000;

    #[test]
    fn divisors_match_the_pl011_formula_at_115200() {
        // 125 MHz / (16 * 115200) = 67.8168: IBRD 67, FBRD round(0.8168 * 64) = 52.
        assert_eq!(baud_divisors(RP2040_PCLK, 115_200), (67, 52));
        // 150 MHz: 81.380 -> IBRD 81, FBRD 24.
        assert_eq!(baud_divisors(RP2350_PCLK, 115_200), (81, 24));
    }

    #[test]
    fn a_zero_or_negative_rate_does_not_divide_by_zero() {
        for pclk in [RP2040_PCLK, RP2350_PCLK] {
            let (ibrd, _) = baud_divisors(pclk, 0);
            assert!(ibrd >= 1);
            let (ibrd, _) = baud_divisors(pclk, -9600);
            assert!(ibrd >= 1);
        }
    }

    #[test]
    fn an_absurd_rate_keeps_ibrd_at_least_one() {
        assert_eq!(baud_divisors(RP2040_PCLK, i32::MAX).0, 1);
        assert_eq!(baud_divisors(RP2040_PCLK, 300).0, 26041);
    }
}
