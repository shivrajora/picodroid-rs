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

/// Set main-axis flex alignment from a Picodroid gravity bitmask.
///
/// We accept Android's gravity constants and translate the main-axis bits
/// (LEFT/CENTER_HORIZONTAL/RIGHT for horizontal flow, TOP/CENTER_VERTICAL/
/// BOTTOM for vertical flow) into LVGL's flex-align enum. The cross-axis
/// stays CENTER for v1; full per-axis routing is part of the
/// resource/inflater milestone.
pub(in crate::system::picodroid::graphics) fn set_gravity(id: i32, gravity: i32) {
    // Mirror the relevant subset of Android's Gravity bits.
    const TOP: i32 = 0x30;
    const BOTTOM: i32 = 0x50;
    const LEFT: i32 = 0x03;
    const RIGHT: i32 = 0x05;
    const CENTER_VERTICAL: i32 = 0x10;
    const CENTER_HORIZONTAL: i32 = 0x01;
    const CENTER: i32 = CENTER_VERTICAL | CENTER_HORIZONTAL;

    let main = if gravity & CENTER == CENTER {
        LV_FLEX_ALIGN_CENTER
    } else if gravity & (LEFT | TOP) != 0 {
        LV_FLEX_ALIGN_START
    } else if gravity & (RIGHT | BOTTOM) != 0 {
        LV_FLEX_ALIGN_END
    } else {
        LV_FLEX_ALIGN_START
    };
    unsafe {
        lv_obj_set_flex_align(
            handle_table::lookup(id),
            main,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );
    }
}
