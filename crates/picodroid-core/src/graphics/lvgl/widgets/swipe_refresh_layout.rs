// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `SwipeRefreshLayout`.
//!
//! Composition: a plain `lv_obj_t` container whose child is the wrapped
//! view, plus a hidden `lv_spinner` parked at the top of the container.
//! The spinner is shown via `setRefreshing(true)` and hidden via
//! `setRefreshing(false)`.
//!
//! Gesture filter: the container registers `LV_EVENT_GESTURE`, but only
//! pull-DOWN gestures (when not currently refreshing) trigger the
//! `fireRefresh()` dispatch. Other directions are silently ignored — apps
//! that want them should use `View.setOnSwipeListener` directly.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

const MAX_LAYOUTS: usize = 4;

#[derive(Copy, Clone)]
struct Slot {
    container: usize,
    spinner: usize,
    /// `true` once setRefreshing(true) (or an auto-trigger pull-down) flips
    /// the spinner on. setRefreshing(false) clears it. The gesture
    /// trampoline ignores pulls while this is set so the listener fires
    /// at most once per refresh cycle.
    refreshing: bool,
}

const EMPTY: Slot = Slot {
    container: 0,
    spinner: 0,
    refreshing: false,
};

const QUEUE_SIZE: usize = 8;

const MAX_LISTENERS: usize = MAX_LAYOUTS;

struct State {
    slots: [Cell<Slot>; MAX_LAYOUTS],
    queue: LocalRing<usize, QUEUE_SIZE>,
    handle_map: PtrMap<MAX_LISTENERS>,
}

// SAFETY: filled by natives and LVGL's gesture callback, drained by natives,
// all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        slots: [const { Cell::new(EMPTY) }; MAX_LAYOUTS],
        queue: LocalRing::new(0),
        handle_map: PtrMap::new(),
    })
};

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    STATE.handle_map.remove(obj)
}

unsafe extern "C" fn gesture_cb(e: *mut lv_event_t) {
    let container = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let indev = lv_indev_active();
        if indev.is_null() {
            return;
        }
        pull_down(container, lv_indev_get_gesture_dir(indev));
    }
}

/// Start a refresh on `container` for a pull-down. Only pull-down
/// (LV_DIR_BOTTOM, finger moving downward) counts, and not while a refresh
/// is already showing. Returns whether the gesture was taken.
unsafe fn pull_down(container: usize, dir: lv_dir_t) -> bool {
    if dir & LV_DIR_BOTTOM == 0 {
        return false;
    }
    for slot in &STATE.slots {
        let s = slot.get();
        if s.container == container {
            if s.refreshing {
                return false;
            }
            slot.set(Slot {
                refreshing: true,
                ..s
            });
            if s.spinner != 0 {
                unsafe { lv_obj_remove_flag(s.spinner as *mut lv_obj_t, LV_OBJ_FLAG_HIDDEN) };
            }
            STATE.queue.push(container);
            return true;
        }
    }
    false
}

/// Android's `onInterceptTouchEvent` for a gesture that landed on a
/// descendant with its own swipe listener: a pull-down inside a
/// SwipeRefreshLayout refreshes, and the child does not see it. Every other
/// direction stays with the child. Returns whether a layout took it.
pub(in crate::graphics) fn intercept(obj: *mut lv_obj_t, dir: lv_dir_t) -> bool {
    unsafe {
        let mut o = lv_obj_get_parent(obj);
        while !o.is_null() {
            if STATE.slots.iter().any(|s| s.get().container == o as usize) {
                return pull_down(o as usize, dir);
            }
            o = lv_obj_get_parent(o);
        }
    }
    false
}

pub(in crate::graphics) fn create() -> i32 {
    unsafe {
        let scr = lifecycle::screen_ptr();
        let container = lv_obj_create(scr);
        lv_obj_set_size(container, 240, 240);
        lv_obj_set_style_pad_left(container, 0, 0);
        lv_obj_set_style_pad_right(container, 0, 0);
        lv_obj_set_style_pad_top(container, 0, 0);
        lv_obj_set_style_pad_bottom(container, 0, 0);
        // Android's SwipeRefreshLayout does not scroll; its child does. The
        // default theme's border leaves a full-size child a few pixels of
        // overflow, and a scrollable container then takes every drag past
        // the scroll limit as an (elastic) scroll, before the gesture below
        // can fire.
        lv_obj_remove_flag(container, LV_OBJ_FLAG_SCROLLABLE);

        // Spinner overlay — pinned to the top center, hidden by default.
        // Shown while a refresh is in flight.
        let spinner = lv_spinner_create(container);
        lv_spinner_set_anim_params(spinner, 1000, 60);
        lv_obj_set_size(spinner, 32, 32);
        lv_obj_set_pos(spinner, 104, 4);
        lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);

        // Stop gestures here instead of passing them up to the screen
        // (LVGL sets GESTURE_BUBBLE on every child; see events/swipe.rs).
        lv_obj_remove_flag(container, LV_OBJ_FLAG_GESTURE_BUBBLE);
        lv_obj_add_event_cb(
            container,
            Some(gesture_cb),
            LV_EVENT_GESTURE,
            core::ptr::null_mut(),
        );

        register_layout(container as usize, spinner as usize);
        handle_table::register(container)
    }
}

pub(in crate::graphics) fn set_refreshing(id: i32, refreshing: bool) {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return;
    }
    let container_ptr = container as usize;
    if let Some(slot) = STATE
        .slots
        .iter()
        .find(|s| s.get().container == container_ptr)
    {
        let s = slot.get();
        slot.set(Slot { refreshing, ..s });
        if s.spinner != 0 {
            let spinner = s.spinner as *mut lv_obj_t;
            unsafe {
                if refreshing {
                    lv_obj_remove_flag(spinner, LV_OBJ_FLAG_HIDDEN);
                } else {
                    lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);
                }
            }
        }
    }
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        match STATE.handle_map.upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("swipe-refresh"),
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

pub fn drain_refresh_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn lookup_refresh_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_swipe_refresh_state() {
    for slot in &STATE.slots {
        slot.set(EMPTY);
    }
    STATE.handle_map.reset();
    STATE.queue.clear();
}

fn register_layout(container: usize, spinner: usize) {
    if let Some(slot) = STATE.slots.iter().find(|s| s.get().container == 0) {
        slot.set(Slot {
            container,
            spinner,
            refreshing: false,
        });
    }
}
