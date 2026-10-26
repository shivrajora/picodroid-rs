// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `LinearLayout`: an `lv_obj` on the layout
//! `graphics/lvgl/layouts` registered for it, whose solver follows Android's
//! `LinearLayout` (weights on the space left over, per-child `layout_gravity`
//! across the axis). Until 2026-10-19 it was LVGL's flex, which has one
//! cross-axis placement per container and a `u8` grow factor.
//!
//! The orientation is kept as the `LV_STYLE_FLEX_FLOW` value (a style slot
//! LVGL reads only on its flex), the gap as the pad row and column, the
//! gravity in this crate's property.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::layouts;
use super::super::lifecycle;
use super::super::style_batch;

pub(in crate::graphics) fn create() -> i32 {
    lifecycle::screen_ptr().map_or(0, create_under)
}

/// [`create`], under `parent` instead of the screen: the inflater makes a
/// node where it belongs, so no re-parenting follows.
#[inline(never)]
pub(in crate::graphics) fn create_under(parent: Obj) -> i32 {
    let Some(o) = lv_obj_create(parent) else {
        return 0;
    };
    // Android LinearLayout never scrolls — use ScrollView for that. Clearing
    // SCROLLABLE also kills the stray scrollbars LVGL would otherwise draw
    // when content brushes the inner edge.
    lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
    // Flat first — no theme border, fill, corners or padding, as on Android
    // (`frame_layout::make_flat`) — since the strip removes local styles
    // too. The strip, the flow and the layout share one refresh
    // (style_batch.rs).
    style_batch::with_one_refresh(o, LV_STYLE_PAD_TOP, || {
        super::frame_layout::make_flat(o);
        set_style_num(o, LV_STYLE_FLEX_FLOW, LV_FLEX_FLOW_COLUMN as i32);
        lv_obj_set_layout(o, layouts::linear_layout_id());
    });
    handle_table::register(o)
}

/// `orientation`: 0 = horizontal (row), non-zero = vertical (column).
pub(in crate::graphics) fn set_orientation(id: i32, orientation: i32) {
    let flow = if orientation == 0 {
        LV_FLEX_FLOW_ROW
    } else {
        LV_FLEX_FLOW_COLUMN
    };
    if let Some(o) = handle_table::get(id) {
        set_style_num(o, LV_STYLE_FLEX_FLOW, flow as i32);
    }
}

/// Place the children from an Android gravity bitmask: the solver reads the
/// bits of the main axis for the run and of the cross axis for each child
/// without a `layout_gravity` of its own, exactly as
/// `android.widget.LinearLayout.setGravity` does.
pub(in crate::graphics) fn set_gravity(id: i32, gravity: i32) {
    if let Some(o) = handle_table::get(id) {
        layouts::set_gravity_prop(o, gravity);
    }
}
