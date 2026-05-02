// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_spi_id: u8) {}

pub fn reconfigure(_spi_id: u8, _freq_hz: u32, _mode: u32) {}

pub fn write_raw(_spi_id: u8, _data: &[u8]) {}

pub fn transfer_raw(_spi_id: u8, tx: &[u8], rx: &mut [u8]) {
    let len = tx.len().min(rx.len());
    rx[..len].copy_from_slice(&tx[..len]);
}
