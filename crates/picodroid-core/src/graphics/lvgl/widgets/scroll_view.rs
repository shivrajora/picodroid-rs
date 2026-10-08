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

unsafe extern "C" fn live_delete_cb(e: *mut lv_event_t) {
    // SAFETY: LVGL calls this with the event of the object being deleted.
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    let mut live = LIVE.get();
    for slot in live.iter_mut() {
        if *slot == obj {
            *slot = 0;
        }
    }
    LIVE.set(live);
}

/// The first live scroller, or null.
pub fn first_live() -> *mut lv_obj_t {
    LIVE.get()
        .iter()
        .find(|s| **s != 0)
        .map_or(core::ptr::null_mut(), |s| *s as *mut lv_obj_t)
}

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        lv_obj_add_event_cb(
            o,
            Some(live_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
        live_add(o as usize);
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
            lv_obj_remove_style(o, core::ptr::null(), LV_PART_SCROLLBAR | LV_STATE_SCROLLED);
            lv_obj_set_style_transition(o, core::ptr::null(), LV_PART_SCROLLBAR);
            lv_obj_set_style_bg_opa(o, LV_OPA_COVER, LV_PART_SCROLLBAR);
        });
        o
    };
    // Let the panel scroll it where the panel can (hw_scroll.rs).
    #[cfg(hw_vscroll)]
    super::super::hw_scroll::watch(ptr);
    handle_table::register(ptr)
}
