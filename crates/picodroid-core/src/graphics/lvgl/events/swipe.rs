// SPDX-License-Identifier: GPL-3.0-only
//! The swipe-listener registry and its event queue.

use super::*;

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
pub(super) static mut SWIPE_QUEUE: [SwipeRecord; SWIPE_QUEUE_SIZE] = [SwipeRecord {
    view_handle: 0,
    direction: 0,
}; SWIPE_QUEUE_SIZE];
pub(super) static mut SWIPE_QUEUE_HEAD: usize = 0;
pub(super) static mut SWIPE_QUEUE_TAIL: usize = 0;

pub(super) static mut VIEW_SWIPE_MAP: PtrMap<MAX_SWIPE_LISTENERS> = PtrMap::new();

pub(super) unsafe extern "C" fn swipe_map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe { map_mut(&raw mut VIEW_SWIPE_MAP).remove(obj) }
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
        let head = SWIPE_QUEUE_HEAD;
        let next = (head + 1) % SWIPE_QUEUE_SIZE;
        if next != SWIPE_QUEUE_TAIL {
            SWIPE_QUEUE[head] = SwipeRecord {
                view_handle: obj,
                direction: dir as i32,
            };
            SWIPE_QUEUE_HEAD = next;
        }
    }
}

pub fn register_view_swipe_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        match map_mut(&raw mut VIEW_SWIPE_MAP).upsert(raw_ptr, obj_ref) {
            Upsert::Updated => return,
            Upsert::Full => {
                warn_full("view-swipe");
                return;
            }
            Upsert::Inserted => {}
        }
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
    unsafe {
        if SWIPE_QUEUE_TAIL == SWIPE_QUEUE_HEAD {
            return None;
        }
        let r = SWIPE_QUEUE[SWIPE_QUEUE_TAIL];
        SWIPE_QUEUE_TAIL = (SWIPE_QUEUE_TAIL + 1) % SWIPE_QUEUE_SIZE;
        Some(r)
    }
}

pub fn lookup_swipe_view_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const VIEW_SWIPE_MAP).lookup(handle) }
}

pub fn reset_view_swipe_listener_state() {
    unsafe {
        map_mut(&raw mut VIEW_SWIPE_MAP).reset();
        SWIPE_QUEUE_HEAD = 0;
        SWIPE_QUEUE_TAIL = 0;
    }
}
