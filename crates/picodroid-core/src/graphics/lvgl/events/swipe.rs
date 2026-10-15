// SPDX-License-Identifier: GPL-3.0-only
//! The swipe-listener registry and its event queue.

use super::*;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

// ── Swipe-listener registry ─────────────────────────────────────────────────
//
// Mirrors the touch-listener pattern: a `(handle, obj_ref)` map keyed by raw
// `lv_obj_t*` plus a small ring buffer of `(handle, lv_dir_t)` records. The
// trampoline reads `lv_indev_active` + `lv_indev_get_gesture_dir` to capture
// the direction, then pushes onto the queue for the framework loop to drain.

pub(super) const MAX_SWIPE_LISTENERS: usize = 32;

#[derive(Copy, Clone)]
pub struct SwipeRecord {
    pub view_handle: usize,
    pub direction: i32,
}

pub(super) const SWIPE_QUEUE_SIZE: usize = 16;

pub(super) struct Swipe {
    queue: LocalRing<SwipeRecord, SWIPE_QUEUE_SIZE>,
    pub(super) view_map: PtrMap<MAX_SWIPE_LISTENERS>,
}

// SAFETY: filled by LVGL's gesture callback and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
pub(super) static SWIPE: Core0<Swipe> = unsafe {
    Core0::new(Swipe {
        queue: LocalRing::new(SwipeRecord {
            view_handle: 0,
            direction: 0,
        }),
        view_map: PtrMap::new(),
    })
};

pub(super) unsafe extern "C" fn swipe_map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    SWIPE.view_map.remove(obj)
}

pub(super) unsafe extern "C" fn swipe_gesture_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let indev = lv_indev_active();
        if indev.is_null() {
            return;
        }
        let dir = lv_indev_get_gesture_dir(indev);
        if dir == LV_DIR_NONE {
            return;
        }
        if super::super::widgets::swipe_refresh_layout::intercept(obj as *mut lv_obj_t, dir) {
            return;
        }
        SWIPE.queue.push(SwipeRecord {
            view_handle: obj,
            direction: dir as i32,
        });
    }
}

pub fn register_view_swipe_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        match SWIPE.view_map.upsert(raw_ptr, obj_ref) {
            Upsert::Updated => return,
            Upsert::Full => {
                warn_full("view-swipe");
                return;
            }
            Upsert::Inserted => {}
        }
        // LVGL sets GESTURE_BUBBLE on every object with a parent, so a
        // gesture climbs from the pressed object to the screen and only the
        // screen hears it. Clearing it here makes this view the target for a
        // swipe that starts on it or on any descendant still bubbling —
        // Android's "the nearest view with a listener".
        lv_obj_remove_flag(raw_obj, LV_OBJ_FLAG_GESTURE_BUBBLE);
        lv_obj_add_event_cb(
            raw_obj,
            Some(swipe_gesture_cb),
            LV_EVENT_GESTURE,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(swipe_map_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
    }
}

pub fn drain_swipe_event() -> Option<SwipeRecord> {
    SWIPE.queue.pop()
}

pub fn lookup_swipe_view_obj(handle: usize) -> Option<u16> {
    SWIPE.view_map.lookup(handle)
}

pub fn reset_view_swipe_listener_state() {
    SWIPE.view_map.reset();
    SWIPE.queue.clear();
}
