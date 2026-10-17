// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ScrollView` (`lv_obj` — scrolls when content exceeds bounds).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::style_batch;
use crate::util::local::Core0;
use core::cell::Cell;

/// The live scrollers, newest last, for the keypad: on a screen with nothing
/// focusable the keys page the first one (`events::keypad::scroll_lone_scroll_view`,
/// docs/designs/app-portability-2026-10.md K7). A slot clears when its
/// scroller is deleted.
const MAX_LIVE: usize = 8;
// SAFETY: widget-layer state, reached only from JVM tasks.
static LIVE: Core0<Cell<[usize; MAX_LIVE]>> = unsafe { Core0::new(Cell::new([0; MAX_LIVE])) };

fn live_add(obj: usize) {
    let mut live = LIVE.get();
    if let Some(slot) = live.iter_mut().find(|s| **s == 0) {
        *slot = obj;
        LIVE.set(live);
    }
}

lv_event_cb! {
    fn live_delete_cb(e) {
        let obj = e.target().addr();
        let mut live = LIVE.get();
        for slot in live.iter_mut() {
            if *slot == obj {
                *slot = 0;
            }
        }
        LIVE.set(live);
    }
}

/// The first live scroller, if any.
pub fn first_live() -> Option<Obj> {
    let addr = LIVE.get().into_iter().find(|&s| s != 0)?;
    // SAFETY: a scroller's address is in `LIVE` from its `create` until
    // `live_delete_cb`, which LVGL runs before it frees the object.
    unsafe { Obj::from_raw(addr as *mut lv_obj_t) }
}

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
    add_event_cb(o, live_delete_cb, LV_EVENT_DELETE);
    live_add(o.addr());
    {
        // Every style set below under one refresh (style_batch.rs).
        style_batch::with_one_refresh(o, LV_STYLE_PAD_TOP, || {
            // Clear theme padding so the scroll container is transparent;
            // padding is controlled explicitly via setPadding().
            lv_obj_set_style_pad_left(o, 0, 0);
            lv_obj_set_style_pad_right(o, 0, 0);
            lv_obj_set_style_pad_top(o, 0, 0);
            lv_obj_set_style_pad_bottom(o, 0, 0);
            // ScrollView is conceptually vertical-only (matches Android, where
            // HorizontalScrollView is a separate class). Without this, LVGL's
            // default elastic over-pull lets users drag horizontally even when
            // content fits, briefly showing a horizontal scrollbar.
            lv_obj_set_scroll_dir(o, LV_DIR_VER);
            // No border and square corners, as Android's ScrollView draws none.
            // The theme's card outline would also cost the hardware scroll path
            // its shortcut: a panel rotating the scroller's rows moves a border
            // line with them, so hw_scroll.rs refuses any scroller that draws
            // one (an app's own GradientDrawable stroke included).
            lv_obj_set_style_border_width(o, 0, 0);
            lv_obj_set_style_radius(o, 0, 0);
            // One scrollbar look, scrolling or not. The theme binds a second
            // style to LV_STATE_SCROLLED (thumb opacity 40% -> 100%) with an
            // 80 ms transition, and every frame of that transition — plus the
            // state change itself, twice per gesture — invalidates the whole
            // scroller: on the touch board that was five full repaints per
            // drag. With no state-bound style a scroll changes no style at all.
            // Opaque rather than 40%, so the thumb is one flat colour: what lets
            // the hardware scroll path repaint only its two ends after a step
            // (hw_scroll.rs) instead of its whole length.
            remove_styles(o, LV_PART_SCROLLBAR | LV_STATE_SCROLLED);
            remove_transition(o, LV_PART_SCROLLBAR);
            lv_obj_set_style_bg_opa(o, LV_OPA_COVER, LV_PART_SCROLLBAR);
        });
    }
    // Let the panel scroll it where the panel can (hw_scroll.rs).
    #[cfg(hw_vscroll)]
    super::super::hw_scroll::watch(o);
    handle_table::register(o)
}
