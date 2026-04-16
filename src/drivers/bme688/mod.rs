//! BME688 environmental sensor driver — temperature, humidity, pressure, gas.
//!
//! Generic over `I2cBus` so the same code works against the RP2040/RP2350 HAL
//! and the sim stub.

mod calib;
mod compensate;

pub use calib::CalibData;

/// A single compensated reading from the BME688.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reading {
    /// Temperature in centi-degrees Celsius (e.g. 2500 = 25.00 °C).
    pub temp_centi_c: i32,
    /// Relative humidity in milli-percent (e.g. 50000 = 50.000 %).
    pub hum_milli_pct: u32,
    /// Barometric pressure in Pascals (e.g. 101325 = 1013.25 hPa).
    pub press_pa: u32,
    /// Gas resistance in Ohms.
    pub gas_ohm: u32,
}

/// Minimal I2C bus trait for the BME688 driver.
pub trait I2cBus {
    fn write(&mut self, addr: u8, data: &[u8]) -> i32;
    fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32;
}

#[derive(Debug)]
pub enum Bme688Error {
    ChipIdMismatch(u8),
    I2cError,
    Timeout,
}

pub struct Bme688<I: I2cBus> {
    bus: I,
    addr: u8,
    calib: CalibData,
    t_fine: i32,
}

impl<I: I2cBus> Bme688<I> {
    pub fn new(mut bus: I, addr: u8) -> Result<Self, Bme688Error> {
        let chip_id = read_reg(&mut bus, addr, 0xD0)?;
        if chip_id != 0x61 {
            return Err(Bme688Error::ChipIdMismatch(chip_id));
        }

        // Read calibration data from two NVM regions
        let mut nvm1 = [0u8; 23]; // 0x8A..0xA0 (23 bytes)
        read_regs(&mut bus, addr, 0x8A, &mut nvm1)?;

        let mut nvm2 = [0u8; 14]; // 0xE1..0xEE (14 bytes)
        read_regs(&mut bus, addr, 0xE1, &mut nvm2)?;

        // res_heat_range at 0x02 bits [5:4]
        let rhr = read_reg(&mut bus, addr, 0x02)?;
        // res_heat_val at 0x00
        let rhv = read_reg(&mut bus, addr, 0x00)?;
        // range_sw_err at 0x04 bits [7:4]
        let rse = read_reg(&mut bus, addr, 0x04)?;

        let calib = CalibData::from_nvm(&nvm1, &nvm2, rhr, rhv, rse);

        Ok(Self {
            bus,
            addr,
            calib,
            t_fine: 0,
        })
    }

    /// Trigger a forced-mode measurement with gas heater enabled.
    pub fn trigger_forced(&mut self) -> Result<(), Bme688Error> {
        // osrs_h = 1x
        write_reg(&mut self.bus, self.addr, 0x72, 0x01)?;
        // run_gas = 1, nb_conv = 0
        write_reg(&mut self.bus, self.addr, 0x71, 0x10)?;
        // osrs_t = 1x, osrs_p = 1x, mode = forced (01)
        write_reg(&mut self.bus, self.addr, 0x74, 0x25)?;
        Ok(())
    }

    /// Poll until measurement is ready. Returns false on timeout.
    pub fn poll_ready(&mut self, max_polls: u32) -> bool {
        for _ in 0..max_polls {
            if let Ok(status) = read_reg(&mut self.bus, self.addr, 0x1D) {
                if status & 0x80 != 0 {
                    return true;
                }
            }
        }
        false
    }

    /// Read raw ADC values and apply compensation.
    pub fn read_compensated(&mut self) -> Result<Reading, Bme688Error> {
        // Read TPH data: 8 bytes from 0x1F (press[3], temp[3], hum[2])
        let mut tph = [0u8; 8];
        read_regs(&mut self.bus, self.addr, 0x1F, &mut tph)?;

        // Read gas data: 2 bytes from 0x2A
        let mut gas = [0u8; 2];
        read_regs(&mut self.bus, self.addr, 0x2A, &mut gas)?;

        let raw_press = ((tph[0] as u32) << 12) | ((tph[1] as u32) << 4) | ((tph[2] as u32) >> 4);
        let raw_temp = ((tph[3] as u32) << 12) | ((tph[4] as u32) << 4) | ((tph[5] as u32) >> 4);
        let raw_hum = ((tph[6] as u16) << 8) | (tph[7] as u16);

        let raw_gas = ((gas[0] as u16) << 2) | ((gas[1] as u16) >> 6);
        let gas_range = gas[1] & 0x0F;

        let temp = compensate::temperature(raw_temp, &self.calib, &mut self.t_fine);
        let press = compensate::pressure(raw_press, &self.calib, self.t_fine);
        let hum = compensate::humidity(raw_hum, &self.calib, self.t_fine);
        let gas_r = compensate::gas(raw_gas, gas_range, &self.calib);

        Ok(Reading {
            temp_centi_c: temp,
            hum_milli_pct: hum,
            press_pa: press,
            gas_ohm: gas_r,
        })
    }
}

fn read_reg<I: I2cBus>(bus: &mut I, addr: u8, reg: u8) -> Result<u8, Bme688Error> {
    bus.write(addr, &[reg]);
    let mut buf = [0u8; 1];
    if bus.read(addr, &mut buf) < 0 {
        return Err(Bme688Error::I2cError);
    }
    Ok(buf[0])
}

fn read_regs<I: I2cBus>(bus: &mut I, addr: u8, reg: u8, buf: &mut [u8]) -> Result<(), Bme688Error> {
    bus.write(addr, &[reg]);
    if bus.read(addr, buf) < 0 {
        return Err(Bme688Error::I2cError);
    }
    Ok(())
}

fn write_reg<I: I2cBus>(bus: &mut I, addr: u8, reg: u8, val: u8) -> Result<(), Bme688Error> {
    if bus.write(addr, &[reg, val]) < 0 {
        return Err(Bme688Error::I2cError);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
