// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_uart_id: u8) {}
pub fn write_byte(_uart_id: u8, byte: u8) {
    print!("{}", byte as char);
}
pub fn read_byte(_uart_id: u8) -> i32 {
    -1
}
