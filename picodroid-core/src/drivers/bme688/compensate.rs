// SPDX-License-Identifier: GPL-3.0-only
//! Bosch BME688 integer compensation formulas.
//!
//! Ported from the Bosch Sensortec BME68x Sensor API (BSD-3-Clause).
//! Uses i64 intermediates where needed to avoid overflow.

use super::CalibData;

/// Compensate temperature. Returns centi-degrees C (e.g. 2500 = 25.00 °C).
/// Also writes back `t_fine` for use by pressure/humidity compensation.
pub fn temperature(raw: u32, c: &CalibData, t_fine: &mut i32) -> i32 {
    let var1 = ((raw as i32) >> 3) - ((c.par_t1 as i32) << 1);
    let var2 = (var1 * (c.par_t2 as i32)) >> 11;
    let var3 = ((var1 >> 1) * (var1 >> 1)) >> 12;
    let var3 = (var3 * ((c.par_t3 as i32) << 4)) >> 14;
    *t_fine = var2 + var3;
    (*t_fine * 5 + 128) >> 8
}

/// Compensate pressure. Returns Pascals.
pub fn pressure(raw: u32, c: &CalibData, t_fine: i32) -> u32 {
    let var1: i32 = (t_fine >> 1) - 64000;
    let var2: i32 = ((((var1 >> 2) * (var1 >> 2)) >> 11) * (c.par_p6 as i32)) >> 2;
    let var2: i32 = var2 + ((var1 * (c.par_p5 as i32)) << 1);
    let var2: i32 = (var2 >> 2) + ((c.par_p4 as i32) << 16);
    let var1: i32 = ((((c.par_p3 as i32) * (((var1 >> 2) * (var1 >> 2)) >> 13)) >> 3)
        + (((c.par_p2 as i32) * var1) >> 1))
        >> 18;
    let var1: i32 = ((32768 + var1) * (c.par_p1 as i32)) >> 15;

    let mut press: u32 = if var1 != 0 {
        let p = 1048576u32.wrapping_sub(raw);
        let p = p.wrapping_sub((var2 as u32) >> 12) * 3125;
        if p >= 0x8000_0000 {
            (p / (var1 as u32)) << 1
        } else {
            (p << 1) / (var1 as u32)
        }
    } else {
        0
    };

    // Widen the squaring + cubing to i64 — at typical sea-level pressure
    // (~101 kPa), `(press >> 8)^3 * par_p10` reaches ≈10 billion, well past
    // the i32 limit, and `(press >> 3)^2` similarly overflows u32. Bosch's
    // reference C uses 32-bit math but relies on undefined-behaviour wrap;
    // i64 here is both correct and accurate.
    let p64 = press as i64;
    let var1 = (((c.par_p9 as i64) * (((p64 >> 3) * (p64 >> 3)) >> 13)) >> 12) as i32;
    let var2 = (((p64 >> 2) * (c.par_p8 as i64)) >> 13) as i32;
    let var3 = (((p64 >> 8) * (p64 >> 8) * (p64 >> 8) * (c.par_p10 as i64)) >> 17) as i32;
    press = (press as i32 + ((var1 + var2 + var3 + ((c.par_p7 as i32) << 7)) >> 4)) as u32;
    press
}

/// Compensate humidity. Returns milli-percent (e.g. 50000 = 50.000 %).
pub fn humidity(raw: u16, c: &CalibData, t_fine: i32) -> u32 {
    let temp_scaled = ((t_fine * 5) + 128) >> 8;

    let var1 =
        (raw as i32) - ((c.par_h1 as i32) * 16) - (((temp_scaled * (c.par_h3 as i32)) / 100) >> 1);
    let var2 = ((c.par_h2 as i32)
        * (((temp_scaled * (c.par_h4 as i32)) / 100)
            + (((temp_scaled * ((temp_scaled * (c.par_h5 as i32)) / 100)) >> 6) / 100)
            + (1 << 14)))
        >> 10;
    let var3 = var1 * var2;
    let var4 = ((c.par_h6 as i32) << 7) + ((temp_scaled * (c.par_h7 as i32)) / 100);
    let var4 = var4 >> 4;
    let var5 = ((var3 >> 14) * (var3 >> 14)) >> 10;
    let var6 = (var4 * var5) >> 1;
    let comp_hum = (((var3 + var6) >> 10) * 1000) >> 12;
    comp_hum.clamp(0, 100_000) as u32
}

/// Compensate gas resistance. Returns Ohms.
pub fn gas(raw: u16, gas_range: u8, c: &CalibData) -> u32 {
    #[rustfmt::skip]
    const LOOKUP_K1: [u32; 16] = [
        2147483647, 2147483647, 2147483647, 2147483647,
        2147483647, 2126008810, 2147483647, 2130303777,
        2147483647, 2147483647, 2143188679, 2136746228,
        2147483647, 2126008810, 2147483647, 2147483647,
    ];
    #[rustfmt::skip]
    const LOOKUP_K2: [u32; 16] = [
        4096000000, 2048000000, 1024000000, 512000000,
        255744255, 127110228, 64000000, 32258064,
        16016016, 8000000, 4000000, 2000000,
        1000000, 500000, 250000, 125000,
    ];

    let var1 =
        ((1340 + (5 * (c.range_sw_err as i64))) * (LOOKUP_K1[gas_range as usize] as i64)) >> 16;
    let var2 = ((raw as i64) << 15) - 16_777_216 + var1;
    let var3 = ((LOOKUP_K2[gas_range as usize] as i64) * var1) >> 9;
    let gas_res = (var3 + (var2 >> 1)) / var2;

    gas_res.max(0) as u32
}
