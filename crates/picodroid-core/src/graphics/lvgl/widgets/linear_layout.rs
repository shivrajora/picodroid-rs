// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `LinearLayout` (LVGL `lv_obj` with flex flow).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        lv_obj_set_flex_flow(o, LV_FLEX_FLOW_COLUMN);
        lv_obj_set_flex_align(
            o,
            LV_FLEX_ALIGN_START,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );
        // Android LinearLayout never scrolls — use ScrollView for that. Clearing
        // SCROLLABLE also kills the stray scrollbars LVGL would otherwise draw
        // when content brushes the inner edge (e.g. the 2 px default border
        // eating into a 224 px child inside a 240 px parent).
        lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
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
pub(in crate::graphics) fn set_orientation(id: i32, orientation: i32) {
    let flow = if orientation == 0 {
        LV_FLEX_FLOW_ROW
    } else {
        LV_FLEX_FLOW_COLUMN
    };
    unsafe { lv_obj_set_flex_flow(handle_table::lookup(id), flow) };
}

/// Gap in pixels between adjacent children. Sets both row and column gap so
/// the value applies whether the layout is later switched to horizontal flow.
pub(in crate::graphics) fn set_spacing(id: i32, spacing: i32) {
    unsafe {
        let o = handle_table::lookup(id);
        lv_obj_set_style_pad_row(o, spacing, 0);
        lv_obj_set_style_pad_column(o, spacing, 0);
    }
}

/// Place the children from an Android gravity bitmask.
///
/// Which of Android's two axes is this layout's main one follows the flow
/// `setOrientation` wrote, which LVGL keeps as a style property — so `RIGHT`
/// aligns a row's children at its right edge and centres a column's children
/// horizontally, exactly as `android.widget.LinearLayout.setGravity` does.
/// `gravity::flex_align` holds the decoding, and its divergences, in one
/// tested place.
pub(in crate::graphics) fn set_gravity(id: i32, gravity: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return; // stale handle — mutating a destroyed View is a no-op
    }
    unsafe {
        let flow = lv_obj_get_style_prop(obj, LV_PART_MAIN, LV_STYLE_FLEX_FLOW).num as u32;
        // Only ROW and COLUMN are ever set, and COLUMN is the low bit of every
        // column flow LVGL defines.
        let (main, cross) = super::gravity::flex_align(gravity, flow & LV_FLEX_FLOW_COLUMN != 0);
        lv_obj_set_flex_align(obj, main, cross, cross);
    }
}
