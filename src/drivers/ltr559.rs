//! LTR-559ALS-01 ambient-light + proximity sensor driver.
//!
//! Generic over `I2cBus` so the same code works against the RP2040/RP2350 HAL
//! and the sim stub. Mirrors the structural pattern in [`crate::drivers::bme688`].

/// Minimal I2C bus trait for the LTR559 driver.
pub trait I2cBus {
    fn write(&mut self, addr: u8, data: &[u8]) -> i32;
    fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32;
}

#[derive(Debug)]
pub enum Ltr559Error {
    PartIdMismatch(u8),
    ManufacturerIdMismatch(u8),
    I2cError,
}

/// One reading from the LTR559.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reading {
    /// Ambient light in milli-lux.
    pub lux_milli: u32,
    /// Raw 11-bit proximity counts.
    pub proximity_raw: u16,
}

// Register map (datasheet rev 1.0).
const REG_ALS_CONTR: u8 = 0x80;
const REG_PS_CONTR: u8 = 0x81;
const REG_PART_ID: u8 = 0x86;
const REG_MANUFAC_ID: u8 = 0x87;
const REG_ALS_DATA_CH1_0: u8 = 0x88; // CH1 low/high then CH0 low/high (4 bytes)
const REG_PS_DATA_0: u8 = 0x8D; // 2 bytes (low, high — high has 3 valid bits)

const PART_ID_EXPECTED: u8 = 0x92;
const MANUFAC_ID_EXPECTED: u8 = 0x05;

pub struct Ltr559<I: I2cBus> {
    bus: I,
    addr: u8,
}

impl<I: I2cBus> Ltr559<I> {
    pub fn new(mut bus: I, addr: u8) -> Result<Self, Ltr559Error> {
        let part = read_reg(&mut bus, addr, REG_PART_ID)?;
        if part != PART_ID_EXPECTED {
            return Err(Ltr559Error::PartIdMismatch(part));
        }
        let manuf = read_reg(&mut bus, addr, REG_MANUFAC_ID)?;
        if manuf != MANUFAC_ID_EXPECTED {
            return Err(Ltr559Error::ManufacturerIdMismatch(manuf));
        }

        // ALS active, gain = 1× (0x01).
        write_reg(&mut bus, addr, REG_ALS_CONTR, 0x01)?;
        // Proximity active, no saturation indication (0x03).
        write_reg(&mut bus, addr, REG_PS_CONTR, 0x03)?;

        Ok(Self { bus, addr })
    }

    /// Read one ambient-light + proximity sample.
    pub fn measure(&mut self) -> Result<Reading, Ltr559Error> {
        let mut als = [0u8; 4];
        read_regs(&mut self.bus, self.addr, REG_ALS_DATA_CH1_0, &mut als)?;
        let ch1 = ((als[1] as u16) << 8) | als[0] as u16;
        let ch0 = ((als[3] as u16) << 8) | als[2] as u16;

        let mut ps = [0u8; 2];
        read_regs(&mut self.bus, self.addr, REG_PS_DATA_0, &mut ps)?;
        let proximity_raw = (((ps[1] & 0x07) as u16) << 8) | ps[0] as u16;

        Ok(Reading {
            lux_milli: compute_lux_milli(ch0, ch1),
            proximity_raw,
        })
    }
}

/// LTR559 lux formula at gain=1×, integration=100 ms.
///
/// Datasheet appendix A: ratio = ch1 / (ch0 + ch1). Different coefficient
/// regions apply per ratio bucket. Implemented in fixed-point milli-lux.
fn compute_lux_milli(ch0: u16, ch1: u16) -> u32 {
    let sum = ch0 as u32 + ch1 as u32;
    if sum == 0 {
        return 0;
    }
    // ratio scaled by 1000
    let ratio_milli = (ch1 as u32 * 1000) / sum;

    let ch0 = ch0 as i64;
    let ch1 = ch1 as i64;

    // Datasheet formulas, scaled by 1000 to keep milli-lux precision.
    // Coefficients are integers ×10000 over the original float values.
    // lux × 1000 = (a*ch0 - b*ch1) / 10
    let lux_milli = if ratio_milli < 450 {
        // 17743 * ch0 + 11059 * ch1, ÷ 10000 → lux. Multiply by 1000 → milli-lux.
        ((17743 * ch0 + 11059 * ch1) / 10) as i64
    } else if ratio_milli < 640 {
        // 42785 * ch0 - 19548 * ch1
        ((42785 * ch0 - 19548 * ch1) / 10) as i64
    } else if ratio_milli < 850 {
        // 5926 * ch0 + 1185 * ch1
        ((5926 * ch0 + 1185 * ch1) / 10) as i64
    } else {
        0
    };

    lux_milli.max(0) as u32
}

fn read_reg<I: I2cBus>(bus: &mut I, addr: u8, reg: u8) -> Result<u8, Ltr559Error> {
    if bus.write(addr, &[reg]) < 0 {
        return Err(Ltr559Error::I2cError);
    }
    let mut buf = [0u8; 1];
    if bus.read(addr, &mut buf) < 0 {
        return Err(Ltr559Error::I2cError);
    }
    Ok(buf[0])
}

fn read_regs<I: I2cBus>(bus: &mut I, addr: u8, reg: u8, buf: &mut [u8]) -> Result<(), Ltr559Error> {
    if bus.write(addr, &[reg]) < 0 {
        return Err(Ltr559Error::I2cError);
    }
    if bus.read(addr, buf) < 0 {
        return Err(Ltr559Error::I2cError);
    }
    Ok(())
}

fn write_reg<I: I2cBus>(bus: &mut I, addr: u8, reg: u8, val: u8) -> Result<(), Ltr559Error> {
    if bus.write(addr, &[reg, val]) < 0 {
        return Err(Ltr559Error::I2cError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CannedI2c {
        last_reg: Option<u8>,
        part_id: u8,
        manuf_id: u8,
        als: [u8; 4],
        ps: [u8; 2],
        write_log: [Option<(u8, u8)>; 8],
        write_log_len: usize,
    }

    impl CannedI2c {
        fn good() -> Self {
            Self {
                last_reg: None,
                part_id: PART_ID_EXPECTED,
                manuf_id: MANUFAC_ID_EXPECTED,
                als: [0; 4],
                ps: [0; 2],
                write_log: [None; 8],
                write_log_len: 0,
            }
        }

        fn logged(&self, reg: u8, val: u8) -> bool {
            self.write_log
                .iter()
                .flatten()
                .any(|&(r, v)| r == reg && v == val)
        }
    }

    impl I2cBus for CannedI2c {
        fn write(&mut self, _addr: u8, data: &[u8]) -> i32 {
            if data.len() == 2 && self.write_log_len < self.write_log.len() {
                self.write_log[self.write_log_len] = Some((data[0], data[1]));
                self.write_log_len += 1;
            }
            if !data.is_empty() {
                self.last_reg = Some(data[0]);
            }
            data.len() as i32
        }
        fn read(&mut self, _addr: u8, buf: &mut [u8]) -> i32 {
            match self.last_reg {
                Some(REG_PART_ID) if !buf.is_empty() => buf[0] = self.part_id,
                Some(REG_MANUFAC_ID) if !buf.is_empty() => buf[0] = self.manuf_id,
                Some(REG_ALS_DATA_CH1_0) if buf.len() >= 4 => buf[..4].copy_from_slice(&self.als),
                Some(REG_PS_DATA_0) if buf.len() >= 2 => buf[..2].copy_from_slice(&self.ps),
                _ => buf.fill(0),
            }
            buf.len() as i32
        }
    }

    #[test]
    fn new_succeeds_with_correct_ids() {
        let bus = CannedI2c::good();
        let drv = Ltr559::new(bus, 0x23);
        assert!(drv.is_ok());
    }

    #[test]
    fn new_rejects_wrong_part_id() {
        let mut bus = CannedI2c::good();
        bus.part_id = 0x00;
        let result = Ltr559::new(bus, 0x23);
        assert!(matches!(result, Err(Ltr559Error::PartIdMismatch(0x00))));
    }

    #[test]
    fn new_rejects_wrong_manufacturer_id() {
        let mut bus = CannedI2c::good();
        bus.manuf_id = 0xAA;
        let result = Ltr559::new(bus, 0x23);
        assert!(matches!(
            result,
            Err(Ltr559Error::ManufacturerIdMismatch(0xAA))
        ));
    }

    #[test]
    fn new_writes_control_registers() {
        let bus = CannedI2c::good();
        let drv = Ltr559::new(bus, 0x23).unwrap();
        assert!(drv.bus.logged(REG_ALS_CONTR, 0x01));
        assert!(drv.bus.logged(REG_PS_CONTR, 0x03));
    }

    #[test]
    fn measure_decodes_proximity_11_bits() {
        let mut bus = CannedI2c::good();
        // PS low byte = 0xAB, high byte = 0x05 (top 5 bits ignored, bottom 3 = 0b101)
        bus.ps = [0xAB, 0x05];
        let mut drv = Ltr559::new(bus, 0x23).unwrap();
        let r = drv.measure().unwrap();
        assert_eq!(r.proximity_raw, (0b101 << 8) | 0xAB);
    }

    #[test]
    fn lux_zero_for_zero_counts() {
        assert_eq!(compute_lux_milli(0, 0), 0);
    }

    #[test]
    fn lux_plausible_for_indoor_counts() {
        // Roughly indoor-office levels: ch0 = 800, ch1 = 200 → ratio ≈ 0.20 (low band).
        let lux_milli = compute_lux_milli(800, 200);
        // Expect somewhere around 1400 lux at the low end.
        assert!(
            lux_milli > 100_000 && lux_milli < 5_000_000,
            "lux_milli={lux_milli} out of plausible indoor range"
        );
    }
}
