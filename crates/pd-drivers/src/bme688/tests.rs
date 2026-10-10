// SPDX-License-Identifier: GPL-3.0-only
use super::compensate;
use super::*;

fn test_calib() -> CalibData {
    CalibData {
        par_t1: 26000,
        par_t2: 26500,
        par_t3: -3,
        par_p1: 36000,
        par_p2: -10400,
        par_p3: 88,
        par_p4: 7500,
        par_p5: -100,
        par_p6: 30,
        par_p7: -30,
        par_p8: -4000,
        par_p9: -2000,
        par_p10: 30,
        par_h1: 800,
        par_h2: 1024,
        par_h3: 0,
        par_h4: 45,
        par_h5: 20,
        par_h6: 120,
        par_h7: -100,
        par_g1: -30,
        par_g2: -5000,
        par_g3: 18,
        res_heat_range: 1,
        res_heat_val: 50,
        range_sw_err: 0,
    }
}

#[test]
fn temperature_compensation_plausible() {
    let c = test_calib();
    let mut t_fine = 0i32;
    let raw = 0x80000; // mid-range 20-bit ADC
    let temp = compensate::temperature(raw, &c, &mut t_fine);
    // Should be a plausible room temperature (0..50 °C = 0..5000 centi-°C)
    assert!(temp > 0 && temp < 5000, "temp={temp} centi-°C out of range");
    assert_ne!(t_fine, 0, "t_fine should be non-zero");
}

#[test]
fn pressure_compensation_plausible() {
    let c = test_calib();
    let mut t_fine = 0i32;
    compensate::temperature(0x80000, &c, &mut t_fine);

    let raw_press = 0x80000;
    let press = compensate::pressure(raw_press, &c, t_fine);
    // Should be somewhere around atmospheric: 80000..120000 Pa
    assert!(
        press > 50000 && press < 150000,
        "pressure={press} Pa out of range"
    );
}

/// Bosch's BME68x `calc_pressure` (32-bit integer variant, the API's
/// reference), transcribed independently of the port so a dropped shift or
/// a wrong constant in `compensate::pressure` shows up as a mismatch. The
/// cubic tail is widened to i64 as the port does (the C relies on wrap).
fn bosch_pressure(pres_adc: u32, c: &CalibData, t_fine: i32) -> u32 {
    let mut var1: i32 = (t_fine >> 1) - 64000;
    let mut var2: i32 = ((((var1 >> 2) * (var1 >> 2)) >> 11) * (c.par_p6 as i32)) >> 2;
    var2 += (var1 * (c.par_p5 as i32)) << 1;
    var2 = (var2 >> 2) + ((c.par_p4 as i32) << 16);
    var1 = (((((var1 >> 2) * (var1 >> 2)) >> 13) * ((c.par_p3 as i32) << 5)) >> 3)
        + (((c.par_p2 as i32) * var1) >> 1);
    var1 >>= 18;
    var1 = ((32768 + var1) * (c.par_p1 as i32)) >> 15;
    let mut p: u32 = 1_048_576u32.wrapping_sub(pres_adc);
    p = p.wrapping_sub((var2 as u32) >> 12).wrapping_mul(3125);
    p = if p >= 0x8000_0000 {
        (p / (var1 as u32)) << 1
    } else {
        (p << 1) / (var1 as u32)
    };
    let p64 = p as i64;
    let v1 = (((c.par_p9 as i64) * (((p64 >> 3) * (p64 >> 3)) >> 13)) >> 12) as i32;
    let v2 = (((p64 >> 2) * (c.par_p8 as i64)) >> 13) as i32;
    let v3 = (((p64 >> 8) * (p64 >> 8) * (p64 >> 8) * (c.par_p10 as i64)) >> 17) as i32;
    (p as i32 + ((v1 + v2 + v3 + ((c.par_p7 as i32) << 7)) >> 4)) as u32
}

#[test]
fn pressure_compensation_matches_bosch_reference() {
    let c = test_calib();
    // Room temperature, then well away from it on both sides, where the
    // par_p3 term (which the port scaled 32x too small) is visible.
    for t_fine in [128_000i32, 40_000, 220_000, -20_000] {
        for raw in [0x80000u32, 0x60000, 0xA0000] {
            assert_eq!(
                compensate::pressure(raw, &c, t_fine),
                bosch_pressure(raw, &c, t_fine),
                "raw {raw:#x} t_fine {t_fine}"
            );
        }
    }
}

#[test]
fn humidity_compensation_plausible() {
    let c = test_calib();
    let mut t_fine = 0i32;
    compensate::temperature(0x80000, &c, &mut t_fine);

    let raw_hum = 0x8000;
    let hum = compensate::humidity(raw_hum, &c, t_fine);
    // Should be 0..100_000 milli-percent
    assert!(hum <= 100_000, "humidity={hum} milli-pct out of range");
}

#[test]
fn gas_compensation_plausible() {
    let c = test_calib();
    let raw_gas = 512;
    let gas_range = 5;
    let gas = compensate::gas(raw_gas, gas_range, &c);
    assert!(gas > 0, "gas resistance should be > 0");
}

#[test]
fn temperature_varies_with_raw() {
    let c = test_calib();
    let mut t1 = 0i32;
    let mut t2 = 0i32;
    let temp_low = compensate::temperature(0x60000, &c, &mut t1);
    let temp_high = compensate::temperature(0xA0000, &c, &mut t2);
    assert!(
        temp_high > temp_low,
        "higher raw should give higher temp: {temp_low} vs {temp_high}"
    );
}

// ── Sim I2C round-trip test ─────────────────────────────────────────────────

struct FakeI2c {
    last_reg: Option<u8>,
}

impl I2cBus for FakeI2c {
    fn write(&mut self, _addr: u8, data: &[u8]) -> i32 {
        if !data.is_empty() {
            self.last_reg = Some(data[0]);
        }
        data.len() as i32
    }

    fn read(&mut self, _addr: u8, buf: &mut [u8]) -> i32 {
        match self.last_reg {
            Some(0xD0) if !buf.is_empty() => buf[0] = 0x61,
            Some(0x1D) if !buf.is_empty() => buf[0] = 0x80, // new_data ready
            Some(0x1F) if buf.len() >= 8 => {
                // press + temp + hum raw data
                buf[0] = 0x50;
                buf[1] = 0x00;
                buf[2] = 0x00;
                buf[3] = 0x80;
                buf[4] = 0x00;
                buf[5] = 0x00;
                buf[6] = 0x70;
                buf[7] = 0x00;
            }
            Some(0x2A) if buf.len() >= 2 => {
                buf[0] = 0x80; // gas_r_msb
                buf[1] = 0x20; // gas_r_lsb | range=0
            }
            _ => buf.fill(0),
        }
        buf.len() as i32
    }
}

#[test]
fn new_reads_chip_id() {
    let bus = FakeI2c { last_reg: None };
    let bme = Bme688::new(bus, 0x77);
    assert!(bme.is_ok(), "Bme688::new should succeed with chip ID 0x61");
}

#[test]
fn new_rejects_wrong_chip_id() {
    struct BadChipI2c;
    impl I2cBus for BadChipI2c {
        fn write(&mut self, _addr: u8, _data: &[u8]) -> i32 {
            1
        }
        fn read(&mut self, _addr: u8, buf: &mut [u8]) -> i32 {
            buf[0] = 0xFF;
            1
        }
    }
    let result = Bme688::new(BadChipI2c, 0x77);
    assert!(matches!(result, Err(Bme688Error::ChipIdMismatch(0xFF))));
}

#[test]
fn read_compensated_returns_reading() {
    let bus = FakeI2c { last_reg: None };
    let mut bme = Bme688::new(bus, 0x77).unwrap();
    bme.trigger_forced().unwrap();
    assert!(bme.poll_ready(10));
    let reading = bme.read_compensated().unwrap();
    // FakeI2c returns zero calibration, so just verify the flow succeeds
    assert_eq!(reading.press_pa, reading.press_pa); // no-panic smoke test
}
