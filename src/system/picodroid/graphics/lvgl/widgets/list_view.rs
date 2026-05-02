// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ListView` (LVGL `lv_list`).

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_list_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn add_item(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_list_add_text(handle_table::lookup(id), buf.as_ptr() as *const c_char) };
}
