//! Simulator stub for the ST7789 display driver.

pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 240;

pub fn init() {
    println!("[sim] Display: ST7789 init ({}x{})", WIDTH, HEIGHT);
}

pub fn set_window(_x0: u16, _y0: u16, _x1: u16, _y1: u16) {}

pub fn write_pixels(_data: &[u8]) {}

pub fn set_backlight(on: bool) {
    println!("[sim] Display: backlight {}", if on { "ON" } else { "OFF" });
}
