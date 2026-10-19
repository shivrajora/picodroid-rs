// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `RelativeLayout`: an `lv_obj` on the layout
//! `graphics/lvgl/layouts` registered for it. The rules live on the
//! children (`layouts::set_relative_rules`); the container carries only its
//! gravity.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::layouts;
use super::super::lifecycle;

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
    super::frame_layout::make_flat(o);
    // Android RelativeLayout never scrolls; the fit check (display.rs) also
    // treats a scrollable object as a scroll view and stops there.
    lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_set_layout(o, layouts::relative_layout_id());
    handle_table::register(o)
}

/// `RelativeLayout.setGravity`: where the children go as a group when
/// their rules leave room (Android's bits).
pub(in crate::graphics) fn set_gravity(id: i32, gravity: i32) {
    if let Some(o) = handle_table::get(id) {
        layouts::set_gravity_prop(o, gravity);
    }
}
