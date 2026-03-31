use pico_jvm::array_heap::ArrayHeap;

pub fn init(_i2c_id: u8) {}

pub fn set_speed(_i2c_id: u8, _hz: u32) {}

pub fn write(i2c_id: u8, address: u32, _data_idx: u16, len: usize, _arrays: &ArrayHeap) -> i32 {
    println!("[sim] I2C{i2c_id} write addr=0x{address:02x} len={len}");
    len as i32
}

pub fn read(i2c_id: u8, address: u32, buf_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
    println!("[sim] I2C{i2c_id} read addr=0x{address:02x} len={len} → zeros");
    for i in 0..len {
        arrays.store(buf_idx, i, 0);
    }
    len as i32
}
