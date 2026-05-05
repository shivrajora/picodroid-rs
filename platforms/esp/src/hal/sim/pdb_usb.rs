// SPDX-License-Identifier: GPL-3.0-only
// pdb_usb is hardware-only; these stubs exist only for module completeness.
#[allow(dead_code)]
pub fn init() {}
#[allow(dead_code)]
pub fn drain_tx() {}
#[allow(dead_code)]
pub fn queue_read_byte() -> u8 {
    0
}
#[allow(dead_code)]
pub fn queue_read_byte_timeout() -> Option<u8> {
    None
}
#[allow(dead_code)]
pub fn queue_read_u32_le() -> u32 {
    0
}
#[allow(dead_code)]
pub fn write_bytes(_data: &[u8]) {}
