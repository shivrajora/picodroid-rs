//! Touch facade — delegates to the generic XPT2046 driver via board config.
//!
//! On boards without a touch controller (`has-touch` feature absent),
//! all functions are no-ops / return `None`.

#[cfg(feature = "has-touch")]
mod inner {
    use crate::boards;
    use core::ptr::addr_of_mut;

    static mut TOUCH: Option<boards::Touch> = None;

    pub fn init() {
        unsafe {
            addr_of_mut!(TOUCH).write(Some(boards::create_touch()));
        }
    }

    pub fn read_point() -> Option<(u16, u16)> {
        unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap().read_point() }
    }

    pub fn read_raw() -> Option<(u16, u16)> {
        unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap().read_raw() }
    }

    pub fn read_raw_unfiltered() -> (u16, u16) {
        unsafe {
            (*addr_of_mut!(TOUCH))
                .as_mut()
                .unwrap()
                .read_raw_unfiltered()
        }
    }

    pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
        unsafe {
            (*addr_of_mut!(TOUCH))
                .as_mut()
                .unwrap()
                .set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
        }
    }

    pub fn set_rejection_range(lo: u16, hi: u16) {
        unsafe {
            (*addr_of_mut!(TOUCH))
                .as_mut()
                .unwrap()
                .set_rejection_range(lo, hi);
        }
    }
}

#[cfg(not(feature = "has-touch"))]
mod inner {
    pub fn init() {}
    pub fn read_point() -> Option<(u16, u16)> {
        None
    }
    pub fn read_raw() -> Option<(u16, u16)> {
        None
    }
    pub fn read_raw_unfiltered() -> (u16, u16) {
        (0, 0)
    }
    pub fn set_calibration(_: u16, _: u16, _: u16, _: u16) {}
    pub fn set_rejection_range(_: u16, _: u16) {}
}

pub use inner::*;
