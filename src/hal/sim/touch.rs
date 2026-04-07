//! Simulator touch backend — maps mouse input from the minifb display window
//! to touch coordinates consumed by LVGL's input device driver.

pub fn init() {
    println!("[sim] Touch: XPT2046 init (mouse emulation)");
}

pub fn is_pressed() -> bool {
    super::display::mouse_state().0
}

pub fn read_point() -> Option<(u16, u16)> {
    let (pressed, x, y) = super::display::mouse_state();
    if pressed {
        Some((x, y))
    } else {
        None
    }
}

pub fn read_raw() -> Option<(u16, u16)> {
    read_point()
}

pub fn read_raw_unfiltered() -> (u16, u16) {
    let (_, x, y) = super::display::mouse_state();
    (x, y)
}

pub fn set_calibration(_cal_x_min: u16, _cal_x_max: u16, _cal_y_min: u16, _cal_y_max: u16) {}

pub fn set_rejection_range(_lo: u16, _hi: u16) {}
