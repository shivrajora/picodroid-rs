// SPDX-License-Identifier: GPL-3.0-only
// ESP32-S3 PDB-USB stub — Milestone 1.
// Real USB-CDC implementation (esp-hal USB-OTG) is Milestone 9.

pub fn init() {}
pub fn drain_tx() {}

pub fn queue_read_byte() -> u8 {
    0
}

pub fn queue_read_byte_timeout() -> Option<u8> {
    None
}

pub fn queue_read_u32_le() -> u32 {
    0
}

pub fn write_bytes(_data: &[u8]) {}
