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

struct State {
    slots: [Cell<Slot>; MAX_LAYOUTS],
    /// Refresh requests, each the container's handle.
    queue: LocalRing<i32, QUEUE_SIZE>,
}

// SAFETY: filled by natives and LVGL's gesture callback, drained by natives,
// all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        slots: [const { Cell::new(EMPTY) }; MAX_LAYOUTS],
        queue: LocalRing::new(0),
    })
};

lv_event_cb! {
    fn gesture_cb(e) {
        let Some(indev) = lv_indev_active() else {
            return;
        };
        pull_down(e.target(), lv_indev_get_gesture_dir(indev));
    }
}

lv_event_cb! {
    /// Release a layout's slot when its container is deleted, so the slot's
    /// spinner address never outlives the spinner (the container's delete
    /// runs before LVGL frees its children) and a recycled address is never
    /// taken for a layout.
    fn slot_delete_cb(e) {
        let container = e.target().addr();
        for slot in &STATE.slots {
            if slot.get().container == container {
                slot.set(EMPTY);
            }
        }
    }
}

/// Start a refresh on `container` for a pull-down. Only pull-down
/// (LV_DIR_BOTTOM, finger moving downward) counts, and not while a refresh
/// is already showing. Returns whether the gesture was taken.
fn pull_down(container: Obj, dir: lv_dir_t) -> bool {
    if dir & LV_DIR_BOTTOM == 0 {
        return false;
    }
    for (i, slot) in STATE.slots.iter().enumerate() {
        let s = slot.get();
        if s.container == container.addr() {
            if s.refreshing {
                return false;
            }
            slot.set(Slot {
                refreshing: true,
                ..s
            });
            if let Some(spinner) = spinner_of(i) {
                lv_obj_remove_flag(spinner, LV_OBJ_FLAG_HIDDEN);
            }
            let handle = handle_table::handle_of(container);
            if handle != 0 {
                STATE.queue.push(handle);
            }
            return true;
        }
    }
    false
}

/// Android's `onInterceptTouchEvent` for a gesture that landed on a
/// descendant with its own swipe listener: a pull-down inside a
/// SwipeRefreshLayout refreshes, and the child does not see it. Every other
/// direction stays with the child. Returns whether a layout took it.
pub(in crate::graphics) fn intercept(obj: Obj, dir: lv_dir_t) -> bool {
    let mut o = lv_obj_get_parent(obj);
    while let Some(p) = o {
        if STATE.slots.iter().any(|s| s.get().container == p.addr()) {
            return pull_down(p, dir);
        }
        o = lv_obj_get_parent(p);
    }
    false
}

/// The spinner of layout slot `idx`.
fn spinner_of(idx: usize) -> Option<Obj> {
    let slot = STATE.slots[idx].get();
    // SAFETY: a slot names the spinner `create` made under its container,
    // from `register_layout` until `slot_delete_cb` clears it, which LVGL
    // runs on the container's delete before it frees the container's
    // children.
    unsafe { Obj::from_raw(slot.spinner as *mut lv_obj_t) }
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(container) = lifecycle::screen_ptr().and_then(|o| lv_obj_create(o)) else {
        return 0;
    };
    {
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
        if let Some(spinner) = spinner {
            // SAFETY: `spinner` was just made by `lv_spinner_create`.
            unsafe { lv_spinner_set_anim_params(spinner, 1000, 60) };
            lv_obj_set_size(spinner, 32, 32);
            lv_obj_set_pos(spinner, 104, 4);
            lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);
        }

        // Stop gestures here instead of passing them up to the screen
        // (LVGL sets GESTURE_BUBBLE on every child; see events/swipe.rs).
        lv_obj_remove_flag(container, LV_OBJ_FLAG_GESTURE_BUBBLE);
        add_event_cb(container, gesture_cb, LV_EVENT_GESTURE);
        add_event_cb(container, slot_delete_cb, LV_EVENT_DELETE);

        register_layout(container.addr(), spinner.map_or(0, Obj::addr));
        handle_table::register(container)
    }
}

pub(in crate::graphics) fn set_refreshing(id: i32, refreshing: bool) {
    let Some(container) = handle_table::get(id) else {
        return;
    };
    let container_ptr = container.addr();
    if let Some(idx) = STATE
        .slots
        .iter()
        .position(|s| s.get().container == container_ptr)
    {
        let slot = &STATE.slots[idx];
        slot.set(Slot {
            refreshing,
            ..slot.get()
        });
        if let Some(spinner) = spinner_of(idx) {
            if refreshing {
                lv_obj_remove_flag(spinner, LV_OBJ_FLAG_HIDDEN);
            } else {
                lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);
            }
        }
    }
}

pub fn drain_refresh_queue() -> Option<i32> {
    STATE.queue.pop()
}

pub fn reset_swipe_refresh_state() {
    for slot in &STATE.slots {
        slot.set(EMPTY);
    }
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
