//! LVGL impl of `EditText` (LVGL `lv_textarea`).

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_textarea_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn set_text(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_textarea_set_text(handle_table::lookup(id), buf.as_ptr() as *const c_char) };
}

pub(in crate::system::picodroid::graphics) fn set_hint(id: i32, hint: &str) {
    let mut buf = [0u8; 128];
    let len = hint.len().min(127);
    buf[..len].copy_from_slice(&hint.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        lv_textarea_set_placeholder_text(handle_table::lookup(id), buf.as_ptr() as *const c_char)
    };
}

/// Read the current textarea content into `dst` (capped at 256 bytes).
/// Returns the byte length written, or `None` if the textarea is empty
/// or LVGL returned a null pointer.
pub(in crate::system::picodroid::graphics) fn get_text(
    id: i32,
    dst: &mut [u8; 256],
) -> Option<usize> {
    let cstr = unsafe { lv_textarea_get_text(handle_table::lookup(id)) };
    if cstr.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *cstr.add(len) != 0 && len < dst.len() {
            len += 1;
        }
    }
    for (i, slot) in dst[..len].iter_mut().enumerate() {
        *slot = unsafe { *cstr.add(i) } as u8;
    }
    Some(len)
}
