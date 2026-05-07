// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `LinearLayout` (LVGL `lv_obj` with flex flow).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        lv_obj_set_flex_flow(o, LV_FLEX_FLOW_COLUMN);
        lv_obj_set_flex_align(
            o,
            LV_FLEX_ALIGN_START,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );
        // Clear theme padding so only explicit setPadding() takes effect.
        lv_obj_set_style_pad_left(o, 0, 0);
        lv_obj_set_style_pad_right(o, 0, 0);
        lv_obj_set_style_pad_top(o, 0, 0);
        lv_obj_set_style_pad_bottom(o, 0, 0);
        lv_obj_set_style_pad_row(o, 0, 0);
        lv_obj_set_style_pad_column(o, 0, 0);
        o
    };
    handle_table::register(ptr)
}

/// `orientation`: 0 = horizontal (row), non-zero = vertical (column).
pub(in crate::system::picodroid::graphics) fn set_orientation(id: i32, orientation: i32) {
    let flow = if orientation == 0 {
        LV_FLEX_FLOW_ROW
    } else {
        LV_FLEX_FLOW_COLUMN
    };
    unsafe { lv_obj_set_flex_flow(handle_table::lookup(id), flow) };
}

/// Gap in pixels between adjacent children. Sets both row and column gap so
/// the value applies whether the layout is later switched to horizontal flow.
pub(in crate::system::picodroid::graphics) fn set_spacing(id: i32, spacing: i32) {
    unsafe {
        let o = handle_table::lookup(id);
        lv_obj_set_style_pad_row(o, spacing, 0);
        lv_obj_set_style_pad_column(o, spacing, 0);
    }
}
