//! LVGL impl of `ProgressBar` (LVGL `lv_bar`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let b = lv_bar_create(lifecycle::screen_ptr());
        lv_bar_set_value(b, 0, LV_ANIM_OFF);
        b
    };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn set_progress(id: i32, value: i32) {
    unsafe { lv_bar_set_value(handle_table::lookup(id), value, LV_ANIM_ON) };
}
