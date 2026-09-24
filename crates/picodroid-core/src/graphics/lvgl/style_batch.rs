// SPDX-License-Identifier: GPL-3.0-only
//! One style refresh for a run of style sets.
//!
//! Every `lv_obj_set_style_*` ends in `lv_obj_refresh_style`: an invalidate,
//! a `STYLE_CHANGED` event, layout-dirty marks on the object and its parent
//! and the ext-draw cache, all of it fetched from XIP flash on the boards.
//! `LinearLayout.nativeCreate`, one create plus six padding sets, cost
//! 1.7 ms that way on the RP2350 (claudeusage D4). LVGL's own
//! `lv_obj_class_init_obj` sidesteps the same cost by switching refreshing
//! off around the theme and the constructor and refreshing once at the end;
//! [`with_one_refresh`] does the same for the sequences this crate writes.

use crate::lvgl_ffi::*;

/// Run `f`'s style sets on `obj` with LVGL's automatic refresh off, then
/// refresh `obj` once as a set of `prop` would have.
///
/// `prop` stands for the whole batch: the refresh reads only a property's
/// flags (layout update, ext-draw update, inheritable), so it must be the
/// property whose flags cover every set in `f` — `LV_STYLE_PAD_TOP` for
/// padding, gaps and flex flow, `LV_STYLE_BORDER_WIDTH` for a fill with a
/// border. It is deliberately not `LV_STYLE_PROP_ANY`: that also walks every
/// descendant, which none of the replaced refreshes did.
///
/// `f` must not create an object (`lv_obj_class_init_obj` switches the
/// refresh back on, and the sets after it refresh one by one again — still
/// correct, no longer batched) and must not set styles on another object
/// (those would go unrefreshed).
///
/// # Safety
/// `obj` must be a live LVGL object, on the UI task like every `lv_*` call.
#[inline]
pub(in crate::graphics) unsafe fn with_one_refresh<R>(
    obj: *mut lv_obj_t,
    prop: lv_style_prop_t,
    f: impl FnOnce() -> R,
) -> R {
    lv_obj_enable_style_refresh(false);
    let r = f();
    lv_obj_enable_style_refresh(true);
    lv_obj_refresh_style(obj, LV_PART_MAIN, prop);
    r
}
