// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `FrameLayout` (plain `lv_obj` — children stack via absolute pos).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_obj_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}
