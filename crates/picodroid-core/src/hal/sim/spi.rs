// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_spi_id: u8) {}

pub fn init_with_pins(_spi_id: u8, _sck: Option<u8>, _mosi: Option<u8>, _miso: Option<u8>) {}

pub fn reconfigure(_spi_id: u8, _freq_hz: u32, _mode: u32) {}

pub fn write_raw(spi_id: u8, data: &[u8]) {
    println!("[sim] SPI{spi_id} write_raw len={}", data.len());
}

pub fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]) {
    println!("[sim] SPI{spi_id} transfer_raw len={}", tx.len());
    // Loopback for sim
    let len = tx.len().min(rx.len());
    rx[..len].copy_from_slice(&tx[..len]);
}
