// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_i2c_id: u8) {}
pub fn set_speed(_i2c_id: u8, _hz: u32) {}

pub fn write_slice(_i2c_id: u8, _address: u8, data: &[u8]) -> i32 {
    data.len() as i32
}

pub fn read_slice(_i2c_id: u8, _address: u8, buf: &mut [u8]) -> i32 {
    buf.fill(0);
    buf.len() as i32
}
