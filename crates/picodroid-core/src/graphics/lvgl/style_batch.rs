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
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering::Relaxed};

/// How many [`with_one_refresh`] batches are open, and the object the
/// outermost one is on. The outermost batch owns the refresh: a batch
/// inside it on the same object (the inflater's node batch around
/// `set_padding`'s) must neither switch the automatic refresh back on
/// early nor refresh on its own; one on another object refreshes that
/// object itself, since the outer refresh will not reach it. Touched by
/// the UI task only, so plain loads and stores count.
static DEPTH: AtomicU8 = AtomicU8::new(0);
static OUTER: AtomicUsize = AtomicUsize::new(0);

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
/// (those would go unrefreshed). Batches nest: an inner one on the same
/// object folds into the outer one's single refresh.
#[inline]
pub(in crate::graphics) fn with_one_refresh<R>(
    obj: Obj,
    prop: lv_style_prop_t,
    f: impl FnOnce() -> R,
) -> R {
    let depth = DEPTH.load(Relaxed);
    DEPTH.store(depth + 1, Relaxed);
    if depth == 0 {
        OUTER.store(obj.addr(), Relaxed);
        lv_obj_enable_style_refresh(false);
    }
    let r = f();
    DEPTH.store(depth, Relaxed);
    if depth == 0 {
        lv_obj_enable_style_refresh(true);
        lv_obj_refresh_style(obj, LV_PART_MAIN, prop);
    } else if OUTER.load(Relaxed) != obj.addr() {
        // Nested on another object: the refresh is a no-op while the
        // automatic one is off, so switch it on for this object alone.
        lv_obj_enable_style_refresh(true);
        lv_obj_refresh_style(obj, LV_PART_MAIN, prop);
        lv_obj_enable_style_refresh(false);
    }
    r
}
