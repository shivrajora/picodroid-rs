// SPDX-License-Identifier: GPL-3.0-only
pub fn init(_spi_id: u8) {}

pub fn reconfigure(_spi_id: u8, _freq_hz: u32, _mode: u32) {}

pub fn write_raw(_spi_id: u8, _data: &[u8]) {}

pub fn transfer_raw(_spi_id: u8, tx: &[u8], rx: &mut [u8]) {
    let len = tx.len().min(rx.len());
    rx[..len].copy_from_slice(&tx[..len]);
}

pub fn transfer(
    spi_id: u8,
    tx_idx: u16,
    rx_idx: u16,
    len: usize,
    arrays: &mut pico_jvm::array_heap::ArrayHeap,
) -> i32 {
    for i in 0..len {
        let byte = arrays.load(tx_idx, i).unwrap_or(0);
        arrays.store(rx_idx, i, byte);
    }
    let _ = spi_id;
    len as i32
}

pub fn write(
    spi_id: u8,
    _data_idx: u16,
    len: usize,
    _arrays: &pico_jvm::array_heap::ArrayHeap,
) -> i32 {
    let _ = spi_id;
    len as i32
}
