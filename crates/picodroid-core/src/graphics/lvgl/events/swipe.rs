// SPDX-License-Identifier: GPL-3.0-only
//! The swipe event queue.

use super::*;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

// ── Swipe events ────────────────────────────────────────────────────────────
//
// A small ring buffer of `(handle, lv_dir_t)` records. The trampoline reads
// `lv_indev_active` + `lv_indev_get_gesture_dir` to capture the direction,
// then pushes onto the queue for the framework loop to drain and dispatch
// to the view the handle table binds to the handle.

#[derive(Copy, Clone)]
pub struct SwipeRecord {
    /// The swiped widget's `nativeHandle`.
    pub view: i32,
    pub direction: i32,
}

pub(super) const SWIPE_QUEUE_SIZE: usize = 16;

pub(super) struct Swipe {
    queue: LocalRing<SwipeRecord, SWIPE_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL's gesture callback and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
pub(super) static SWIPE: Core0<Swipe> = unsafe {
    Core0::new(Swipe {
        queue: LocalRing::new(SwipeRecord {
            view: 0,
            direction: 0,
        }),
    })
};

lv_event_cb! {
    pub(super) fn swipe_gesture_cb(e) {
        let obj = e.target();
        let Some(indev) = lv_indev_active() else {
            return;
        };
        let dir = lv_indev_get_gesture_dir(indev);
        if dir == LV_DIR_NONE {
            return;
        }
        if super::super::widgets::swipe_refresh_layout::intercept(obj, dir) {
            return;
        }
        let view = super::super::handle_table::handle_of(obj);
        if view == 0 {
            return;
        }
        SWIPE.queue.push(SwipeRecord {
            view,
            direction: dir as i32,
        });
    }
}

/// `View.setOnSwipeListener`'s widget half: make `id`'s widget the target
/// of the gestures that start on it and route them to the swipe queue.
/// Idempotent, and a no-op for a released view.
pub fn attach_swipe(id: i32) {
    let Some(raw_obj) = super::super::handle_table::get(id) else {
        return;
    };
    // LVGL sets GESTURE_BUBBLE on every object with a parent, so a
    // gesture climbs from the pressed object to the screen and only the
    // screen hears it. Clearing it here makes this view the target for a
    // swipe that starts on it or on any descendant still bubbling —
    // Android's "the nearest view with a listener".
    lv_obj_remove_flag(raw_obj, LV_OBJ_FLAG_GESTURE_BUBBLE);
    lv_obj_remove_event_cb(raw_obj, Some(swipe_gesture_cb));
    add_event_cb(raw_obj, swipe_gesture_cb, LV_EVENT_GESTURE);
}

pub fn drain_swipe_event() -> Option<SwipeRecord> {
    SWIPE.queue.pop()
}

pub fn reset_view_swipe_listener_state() {
    SWIPE.queue.clear();
}
