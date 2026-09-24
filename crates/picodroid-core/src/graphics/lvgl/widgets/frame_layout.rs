// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `FrameLayout` (plain `lv_obj` — children stack via absolute pos).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

/// Strip the theme's card look from a freshly created container so it starts as an Android
/// `ViewGroup` does: no background, no border, square corners, zero padding. One
/// `lv_obj_remove_style` with a null style drops every style the theme attached, in one style
/// refresh, where the six per-side `lv_obj_set_style_pad_*` calls this replaced cost one each
/// (1.6–2.0 ms per `LinearLayout` on the RP2350, D4 in
/// docs/designs/claudeusage-gaps-roadmap-2026-09.md). With no style left, LVGL's defaults are the
/// Android ones — `bg_opa` transparent, `border_width` 0, `radius` 0, pads 0 — and a label inside
/// still finds its text colour by inheritance from the screen. Local styles set afterwards
/// (`setPadding`, a `GradientDrawable`, flex flow) apply on top as before. Until 2026-09-24 every
/// container drew the theme's 2 px border, rounded corners and card fill, which every app then
/// stripped by hand.
pub(in crate::graphics::lvgl::widgets) fn make_flat(o: *mut lv_obj_t) {
    unsafe { lv_obj_remove_style(o, core::ptr::null(), LV_PART_ANY | LV_STATE_ANY) };
}

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        make_flat(o);
        // Android FrameLayout never scrolls — use ScrollView for that.
        lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
        o
    };
    handle_table::register(ptr)
}
