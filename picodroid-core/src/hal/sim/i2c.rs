// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::array_heap::ArrayHeap;

use core::cell::RefCell;

pub fn init(_i2c_id: u8) {}

pub fn set_speed(_i2c_id: u8, _hz: u32) {}

pub fn write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32 {
    SIM_I2C_STATE.with_borrow_mut(|s| {
        s.last_reg = data.first().copied();
    });
    println!(
        "[sim] I2C{i2c_id} write_slice addr=0x{address:02x} len={}",
        data.len()
    );
    data.len() as i32
}

pub fn read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32 {
    let reg = SIM_I2C_STATE.with_borrow(|s| s.last_reg);
    println!(
        "[sim] I2C{i2c_id} read_slice addr=0x{address:02x} len={} reg={:?}",
        buf.len(),
        reg,
    );
    sim_bme688_respond(address, reg, buf);
    buf.len() as i32
}

pub fn write(i2c_id: u8, address: u32, _data_idx: u16, len: usize, _arrays: &ArrayHeap) -> i32 {
    println!("[sim] I2C{i2c_id} write addr=0x{address:02x} len={len}");
    len as i32
}

pub fn read(i2c_id: u8, address: u32, buf_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
    println!("[sim] I2C{i2c_id} read addr=0x{address:02x} len={len} → zeros");
    for i in 0..len {
        arrays.store(buf_idx, i, 0);
    }
    len as i32
}

// ── Sim-only BME688 fake ────────────────────────────────────────────────────

struct SimI2cState {
    last_reg: Option<u8>,
}

thread_local! {
    static SIM_I2C_STATE: RefCell<SimI2cState> = const { RefCell::new(SimI2cState { last_reg: None }) };
}

fn sim_bme688_respond(address: u8, reg: Option<u8>, buf: &mut [u8]) {
    if address != 0x77 {
        buf.fill(0);
        return;
    }
    match reg {
        Some(0xD0) if !buf.is_empty() => {
            buf[0] = 0x61;
        }
        Some(0x1D) if !buf.is_empty() => {
            buf[0] = 0x80;
        }
        Some(0x1F) if buf.len() >= 8 => {
            // raw_temp=0x80000 (20-bit), raw_press=0x80000, raw_hum=0x8000
            buf[0] = 0x08; // press_msb
            buf[1] = 0x00; // press_lsb
            buf[2] = 0x00; // press_xlsb
            buf[3] = 0x08; // temp_msb
            buf[4] = 0x00; // temp_lsb
            buf[5] = 0x00; // temp_xlsb
            buf[6] = 0x80; // hum_msb
            buf[7] = 0x00; // hum_lsb
        }
        Some(0x2A) if buf.len() >= 2 => {
            buf[0] = 0x80; // gas_r_msb
            buf[1] = 0x00; // gas_r_lsb (range=0)
        }
        Some(0x8A) => {
            buf.fill(0);
        }
        Some(0xE1) => {
            buf.fill(0);
        }
        _ => {
            buf.fill(0);
        }
    }
}
