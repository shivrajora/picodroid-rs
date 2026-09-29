// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `SeekBar` (LVGL `lv_slider`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 16;
// SAFETY: filled by LVGL callbacks and drained by natives, all on JVM tasks.
static QUEUE: Core0<LocalRing<usize, QUEUE_SIZE>> = unsafe { Core0::new(LocalRing::new(0)) };

/// Press/release edges for onStartTrackingTouch/onStopTrackingTouch —
/// `(slider ptr, started)` where `started` is true on LV_EVENT_PRESSED.
// SAFETY: filled by LVGL callbacks and drained by natives, all on JVM tasks.
static TRACK_QUEUE: Core0<LocalRing<(usize, bool), QUEUE_SIZE>> =
    unsafe { Core0::new(LocalRing::new((0, false))) };

const MAX_LISTENERS: usize = 32;
// SAFETY: a listener registry, reached only from JVM tasks.
static HANDLE_MAP: Core0<PtrMap<MAX_LISTENERS>> = unsafe { Core0::new(PtrMap::new()) };

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    HANDLE_MAP.remove(obj)
}

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    QUEUE.push(obj as usize);
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
    TRACK_QUEUE.push((handle, started));
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
    let obj = handle_table::lookup(id);
    unsafe {
        lv_slider_set_range(obj, 0, max);
        // Android's `setMax` pulls a progress past the new maximum down to
        // it; LVGL leaves the value where it was (QA 2026-09-13).
        if lv_slider_get_value(obj) > max {
            lv_slider_set_value(obj, max, LV_ANIM_OFF);
        }
    }
}

pub(in crate::graphics) fn set_progress(id: i32, progress: i32) {
    // Instant, as Android's `setProgress` is. Animated, LVGL reports the
    // animation's target from `get_value` until the animation ends and its
    // `set_range` clamp misses the in-flight value, so `getProgress()`
    // right after `setProgress()`/`setMax()` disagreed with the caller.
    unsafe { lv_slider_set_value(handle_table::lookup(id), progress, LV_ANIM_OFF) };
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
        match HANDLE_MAP.upsert(raw_ptr, obj_ref) {
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

pub fn drain_seek_change_queue() -> Option<usize> {
    QUEUE.pop()
}

pub fn drain_seek_tracking_queue() -> Option<(usize, bool)> {
    TRACK_QUEUE.pop()
}

pub fn lookup_seek_bar_obj(handle: usize) -> Option<u16> {
    HANDLE_MAP.lookup(handle)
}

pub fn reset_seek_bar_state() {
    HANDLE_MAP.reset();
    QUEUE.clear();
    TRACK_QUEUE.clear();
}
