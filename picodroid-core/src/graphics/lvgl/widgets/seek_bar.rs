// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `SeekBar` (LVGL `lv_slider`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{map_mut, map_ref, warn_full, PtrMap, Upsert};

const QUEUE_SIZE: usize = 16;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

/// Press/release edges for onStartTrackingTouch/onStopTrackingTouch —
/// `(slider ptr, started)` where `started` is true on LV_EVENT_PRESSED.
static mut TRACK_QUEUE: [(usize, bool); QUEUE_SIZE] = [(0, false); QUEUE_SIZE];
static mut TRACK_HEAD: usize = 0;
static mut TRACK_TAIL: usize = 0;

const MAX_LISTENERS: usize = 32;
static mut HANDLE_MAP: PtrMap<MAX_LISTENERS> = PtrMap::new();

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe { map_mut(&raw mut HANDLE_MAP).remove(obj) }
}

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    unsafe {
        let next = (QUEUE_HEAD + 1) % QUEUE_SIZE;
        if next != QUEUE_TAIL {
            QUEUE[QUEUE_HEAD] = obj as usize;
            QUEUE_HEAD = next;
        }
    }
}

unsafe extern "C" fn pressed_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    enqueue_track(obj as usize, true);
}

unsafe extern "C" fn released_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    enqueue_track(obj as usize, false);
}

fn enqueue_track(handle: usize, started: bool) {
    unsafe {
        let next = (TRACK_HEAD + 1) % QUEUE_SIZE;
        if next != TRACK_TAIL {
            TRACK_QUEUE[TRACK_HEAD] = (handle, started);
            TRACK_HEAD = next;
        }
    }
}

fn create_internal(max: i32) -> i32 {
    let ptr = unsafe {
        let s = lv_slider_create(lifecycle::screen_ptr());
        lv_slider_set_range(s, 0, max);
        lv_slider_set_value(s, 0, LV_ANIM_OFF);
        lv_obj_add_event_cb(
            s,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(s, Some(pressed_cb), LV_EVENT_PRESSED, core::ptr::null_mut());
        lv_obj_add_event_cb(
            s,
            Some(released_cb),
            LV_EVENT_RELEASED,
            core::ptr::null_mut(),
        );
        s
    };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn create() -> i32 {
    create_internal(100)
}

pub(in crate::graphics) fn create_with_max(max: i32) -> i32 {
    create_internal(max)
}

pub(in crate::graphics) fn set_max(id: i32, max: i32) {
    unsafe { lv_slider_set_range(handle_table::lookup(id), 0, max) };
}

pub(in crate::graphics) fn set_progress(id: i32, progress: i32) {
    unsafe { lv_slider_set_value(handle_table::lookup(id), progress, LV_ANIM_ON) };
}

pub(in crate::graphics) fn get_progress(id: i32) -> i32 {
    unsafe { lv_slider_get_value(handle_table::lookup(id)) }
}

pub(in crate::graphics) fn perform_progress_change(id: i32) {
    unsafe {
        let obj = handle_table::lookup(id);
        let cur = lv_slider_get_value(obj);
        let next = cur.saturating_add(1);
        lv_slider_set_value(obj, next, LV_ANIM_OFF);
        lv_obj_send_event(obj, LV_EVENT_VALUE_CHANGED, core::ptr::null_mut());
    }
}

/// Synthetically fire a press/release pair through the real LVGL event
/// callbacks — headless-testing counterpart of `perform_progress_change`.
pub(in crate::graphics) fn perform_tracking_touch(id: i32) {
    unsafe {
        let obj = handle_table::lookup(id);
        lv_obj_send_event(obj, LV_EVENT_PRESSED, core::ptr::null_mut());
        lv_obj_send_event(obj, LV_EVENT_RELEASED, core::ptr::null_mut());
    }
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        match map_mut(&raw mut HANDLE_MAP).upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("seek-bar"),
            Upsert::Inserted => {
                // Unregister on widget delete so a recycled lv_obj address
                // can't alias a dead widget's listener entry.
                lv_obj_add_event_cb(
                    raw_ptr as *mut lv_obj_t,
                    Some(map_delete_cb),
                    LV_EVENT_DELETE,
                    core::ptr::null_mut(),
                );
            }
        }
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_seek_change_queue() -> Option<usize> {
    unsafe {
        if QUEUE_TAIL == QUEUE_HEAD {
            return None;
        }
        let h = QUEUE[QUEUE_TAIL];
        QUEUE_TAIL = (QUEUE_TAIL + 1) % QUEUE_SIZE;
        Some(h)
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_seek_tracking_queue() -> Option<(usize, bool)> {
    unsafe {
        if TRACK_TAIL == TRACK_HEAD {
            return None;
        }
        let e = TRACK_QUEUE[TRACK_TAIL];
        TRACK_TAIL = (TRACK_TAIL + 1) % QUEUE_SIZE;
        Some(e)
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_seek_bar_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const HANDLE_MAP).lookup(handle) }
}

pub fn reset_seek_bar_state() {
    unsafe {
        map_mut(&raw mut HANDLE_MAP).reset();
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
        TRACK_HEAD = 0;
        TRACK_TAIL = 0;
    }
}
