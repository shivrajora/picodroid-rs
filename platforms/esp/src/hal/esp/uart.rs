// SPDX-License-Identifier: GPL-3.0-only
// ESP32-S3 UART stub — Milestone 1. Real esp-hal UART wiring is Milestone 2.

pub fn init(_uart_id: u8) {}

pub fn reconfigure(
    _uart_id: u8,
    _baudrate: i32,
    _data_size: i32,
    _parity: i32,
    _stop_bits: i32,
    _hw_flow: i32,
) {
}

pub fn write_byte(_uart_id: u8, _byte: u8) {}

pub fn read_byte(_uart_id: u8) -> i32 {
    -1
}
