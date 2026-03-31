use pico_jvm::array_heap::ArrayHeap;

pub fn init(_spi_id: u8) {}

pub fn reconfigure(_spi_id: u8, _freq_hz: u32, _mode: u32) {}

/// Full-duplex loopback: echoes tx into rx.
pub fn transfer(spi_id: u8, tx_idx: u16, rx_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
    println!("[sim] SPI{spi_id} transfer len={len} (loopback)");
    for i in 0..len {
        let byte = arrays.load(tx_idx, i).unwrap_or(0);
        arrays.store(rx_idx, i, byte);
    }
    len as i32
}

pub fn write(spi_id: u8, _data_idx: u16, len: usize, _arrays: &ArrayHeap) -> i32 {
    println!("[sim] SPI{spi_id} write len={len}");
    len as i32
}
