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

pub fn read_raw() -> Option<(u16, u16)> {
    None
}

pub fn read_raw_unfiltered() -> (u16, u16) {
    (0, 0)
}

pub fn set_calibration(_cal_x_min: u16, _cal_x_max: u16, _cal_y_min: u16, _cal_y_max: u16) {}

pub fn set_rejection_range(_lo: u16, _hi: u16) {}
