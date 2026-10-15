// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `SeekBar` (LVGL `lv_slider`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::PtrMap;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 16;
struct State {
    queue: LocalRing<usize, QUEUE_SIZE>,
    /// Press/release edges for onStartTrackingTouch/onStopTrackingTouch —
    /// `(slider ptr, started)` where `started` is true on LV_EVENT_PRESSED.
    track_queue: LocalRing<(usize, bool), QUEUE_SIZE>,
    handle_map: PtrMap<MAX_LISTENERS>,
}

// SAFETY: filled by LVGL's slider callbacks and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        queue: LocalRing::new(0),
        track_queue: LocalRing::new((0, false)),
        handle_map: PtrMap::new(),
    })
};

const MAX_LISTENERS: usize = 32;

lv_event_cb! {
    fn map_delete_cb(e) {
        let obj = e.target().addr();
        STATE.handle_map.remove(obj)
    }
}

lv_event_cb! {
    fn value_changed_cb(e) {
        let obj = e.target();
        STATE.queue.push(obj.addr());
    }
}

lv_event_cb! {
    fn pressed_cb(e) {
        let obj = e.target();
        enqueue_track(obj.addr(), true);
    }
}

lv_event_cb! {
    fn released_cb(e) {
        let obj = e.target();
        enqueue_track(obj.addr(), false);
    }
}

fn enqueue_track(handle: usize, started: bool) {
    STATE.track_queue.push((handle, started));
}

fn create_internal(max: i32) -> i32 {
    let Some(s) = lifecycle::screen_ptr().and_then(|o| lv_slider_create(o)) else {
        return 0;
    };
    // SAFETY: `s` was just made by `lv_slider_create`.
    unsafe {
        lv_slider_set_range(s, 0, max);
        lv_slider_set_value(s, 0, LV_ANIM_OFF);
    }
    add_event_cb(s, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    add_event_cb(s, pressed_cb, LV_EVENT_PRESSED);
    add_event_cb(s, released_cb, LV_EVENT_RELEASED);
    handle_table::register(s)
}

pub(in crate::graphics) fn create() -> i32 {
    create_internal(100)
}

pub(in crate::graphics) fn create_with_max(max: i32) -> i32 {
    create_internal(max)
}

pub(in crate::graphics) fn set_max(id: i32, max: i32) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    // SAFETY: a `SeekBar` native: its constructors give `View` the
    // `lv_slider` `create_internal` made, and no other handle.
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
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    // SAFETY: a `SeekBar` native: its constructors give `View` the
    // `lv_slider` `create_internal` made, and no other handle.
    unsafe { lv_slider_set_value(obj, progress, LV_ANIM_OFF) };
}

/// The slider's value; 0 for a stale handle.
pub(in crate::graphics) fn get_progress(id: i32) -> i32 {
    let Some(obj) = handle_table::get(id) else {
        return 0;
    };
    // SAFETY: a `SeekBar` native: its constructors give `View` the
    // `lv_slider` `create_internal` made, and no other handle.
    unsafe { lv_slider_get_value(obj) }
}

pub(in crate::graphics) fn perform_progress_change(id: i32) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    // SAFETY: a `SeekBar` native: its constructors give `View` the
    // `lv_slider` `create_internal` made, and no other handle.
    unsafe {
        let next = lv_slider_get_value(obj).saturating_add(1);
        lv_slider_set_value(obj, next, LV_ANIM_OFF);
    }
    send_event(obj, Notice::ValueChanged);
}

/// Synthetically fire a press/release pair through the real LVGL event
/// callbacks — headless-testing counterpart of `perform_progress_change`.
pub(in crate::graphics) fn perform_tracking_touch(id: i32) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    send_event(obj, Notice::Pressed);
    send_event(obj, Notice::Released);
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    super::compound::register_listener_with_delete_hook(
        &STATE.handle_map,
        id,
        obj_ref,
        map_delete_cb,
        "seek-bar",
    );
}

pub fn drain_seek_change_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn drain_seek_tracking_queue() -> Option<(usize, bool)> {
    STATE.track_queue.pop()
}

pub fn lookup_seek_bar_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_seek_bar_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
    STATE.track_queue.clear();
}
