//! Touch facade — delegates to the generic XPT2046 driver via board config.
//!
//! Preserves the free-function API (`hal::touch::init()`, etc.) so that
//! `engine.rs` and LVGL callbacks need zero changes.

use crate::boards;
use core::ptr::addr_of_mut;

static mut TOUCH: Option<boards::Touch> = None;

/// Initialize the touch controller via the board-specific driver.
pub fn init() {
    unsafe {
        addr_of_mut!(TOUCH).write(Some(boards::create_touch()));
    }
}

/// Read calibrated screen coordinates (0..WIDTH-1, 0..HEIGHT-1).
/// Returns `None` if no touch is active.
pub fn read_point() -> Option<(u16, u16)> {
    unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap().read_point() }
}

/// Read raw 12-bit ADC values (for calibration).
/// Returns `None` if no touch is active (rejected by noise thresholds).
pub fn read_raw() -> Option<(u16, u16)> {
    unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap().read_raw() }
}

/// Read raw 12-bit ADC values without noise rejection.
/// Always returns a value — useful for noise-floor discovery.
pub fn read_raw_unfiltered() -> (u16, u16) {
    unsafe {
        (*addr_of_mut!(TOUCH))
            .as_mut()
            .unwrap()
            .read_raw_unfiltered()
    }
}

/// Update the touch calibration constants at runtime.
pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
    unsafe {
        (*addr_of_mut!(TOUCH))
            .as_mut()
            .unwrap()
            .set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
    }
}

/// Update noise-rejection thresholds at runtime.
pub fn set_rejection_range(lo: u16, hi: u16) {
    unsafe {
        (*addr_of_mut!(TOUCH))
            .as_mut()
            .unwrap()
            .set_rejection_range(lo, hi);
    }
}
