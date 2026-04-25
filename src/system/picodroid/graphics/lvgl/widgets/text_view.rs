//! LVGL impl of `TextView` (LVGL `lv_label`).

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

/// Create an `lv_label` on the active screen and register its handle.
pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_label_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn set_text(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_label_set_text(handle_table::lookup(id), buf.as_ptr() as *const c_char) };
}

pub(in crate::system::picodroid::graphics) fn set_text_color(id: i32, argb: u32) {
    let color = lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    };
    unsafe { lv_obj_set_style_text_color(handle_table::lookup(id), color, 0) };
}
