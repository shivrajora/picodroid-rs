// SPDX-License-Identifier: GPL-3.0-only
// ESP32-S3 UART stub — Milestone 1. Real esp-hal UART wiring is Milestone 2.

pub fn init(_uart_id: u8) {}

pub fn write_byte(_uart_id: u8, _byte: u8) {}

pub fn read_byte(_uart_id: u8) -> i32 {
    -1
}
