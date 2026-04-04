//! Simulator stub for the XPT2046 touch controller.

pub fn init() {
    println!("[sim] Touch: XPT2046 init");
}

pub fn is_pressed() -> bool {
    false
}

pub fn read_point() -> Option<(u16, u16)> {
    None
}
