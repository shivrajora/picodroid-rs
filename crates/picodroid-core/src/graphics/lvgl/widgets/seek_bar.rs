// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `SeekBar` (LVGL `lv_slider`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 16;
struct State {
    /// Value changes, each the slider's handle.
    queue: LocalRing<i32, QUEUE_SIZE>,
    /// Press/release edges for onStartTrackingTouch/onStopTrackingTouch —
    /// `(slider handle, started)` where `started` is true on LV_EVENT_PRESSED.
    track_queue: LocalRing<(i32, bool), QUEUE_SIZE>,
}

// SAFETY: filled by LVGL's slider callbacks and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        queue: LocalRing::new(0),
        track_queue: LocalRing::new((0, false)),
    })
};

lv_event_cb! {
    fn value_changed_cb(e) {
        let handle = handle_table::handle_of(e.target());
        if handle != 0 {
            STATE.queue.push(handle);
        }
    }
}

lv_event_cb! {
    fn pressed_cb(e) {
        enqueue_track(e.target(), true);
    }
}

lv_event_cb! {
    fn released_cb(e) {
        enqueue_track(e.target(), false);
    }
}

fn enqueue_track(obj: Obj, started: bool) {
    let handle = handle_table::handle_of(obj);
    if handle != 0 {
        STATE.track_queue.push((handle, started));
    }
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

pub fn drain_seek_change_queue() -> Option<i32> {
    STATE.queue.pop()
}

pub fn drain_seek_tracking_queue() -> Option<(i32, bool)> {
    STATE.track_queue.pop()
}

pub fn reset_seek_bar_state() {
    STATE.queue.clear();
    STATE.track_queue.clear();
}
