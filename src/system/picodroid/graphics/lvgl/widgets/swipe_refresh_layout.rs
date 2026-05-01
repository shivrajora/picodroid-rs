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

static mut SLOTS: [Slot; MAX_LAYOUTS] = [EMPTY; MAX_LAYOUTS];

const QUEUE_SIZE: usize = 8;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

const MAX_LISTENERS: usize = MAX_LAYOUTS;
static mut HANDLE_MAP: [(usize, u16); MAX_LISTENERS] = [(0, 0); MAX_LISTENERS];
static mut HANDLE_MAP_LEN: usize = 0;

unsafe extern "C" fn gesture_cb(e: *mut lv_event_t) {
    let container = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let indev = lv_indev_active();
        if indev.is_null() {
            return;
        }
        let dir = lv_indev_get_gesture_dir(indev);
        if dir & LV_DIR_BOTTOM == 0 {
            // Only pull-down (LV_DIR_BOTTOM, swipe finger downward) triggers
            // a refresh. Other directions fall through to the generic
            // OnSwipeListener path on the parent View if registered.
            return;
        }
        for slot in &mut SLOTS[..] {
            if slot.container == container {
                if slot.refreshing {
                    return;
                }
                slot.refreshing = true;
                if slot.spinner != 0 {
                    lv_obj_remove_flag(slot.spinner as *mut lv_obj_t, LV_OBJ_FLAG_HIDDEN);
                }
                let next = (QUEUE_HEAD + 1) % QUEUE_SIZE;
                if next != QUEUE_TAIL {
                    QUEUE[QUEUE_HEAD] = container;
                    QUEUE_HEAD = next;
                }
                return;
            }
        }
    }
}

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    unsafe {
        let scr = lifecycle::screen_ptr();
        let container = lv_obj_create(scr);
        lv_obj_set_size(container, 240, 240);
        lv_obj_set_style_pad_left(container, 0, 0);
        lv_obj_set_style_pad_right(container, 0, 0);
        lv_obj_set_style_pad_top(container, 0, 0);
        lv_obj_set_style_pad_bottom(container, 0, 0);

        // Spinner overlay — pinned to the top center, hidden by default.
        // Shown while a refresh is in flight.
        let spinner = lv_spinner_create(container);
        lv_spinner_set_anim_params(spinner, 1000, 60);
        lv_obj_set_size(spinner, 32, 32);
        lv_obj_set_pos(spinner, 104, 4);
        lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);

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

pub(in crate::system::picodroid::graphics) fn add_view(parent: i32, child: i32) {
    let parent_obj = handle_table::lookup(parent);
    let child_obj = handle_table::lookup(child);
    if parent_obj.is_null() || child_obj.is_null() {
        return;
    }
    unsafe { lv_obj_set_parent(child_obj, parent_obj) };
}

pub(in crate::system::picodroid::graphics) fn set_refreshing(id: i32, refreshing: bool) {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return;
    }
    let container_ptr = container as usize;
    unsafe {
        for slot in &mut SLOTS[..] {
            if slot.container == container_ptr {
                slot.refreshing = refreshing;
                if slot.spinner != 0 {
                    let s = slot.spinner as *mut lv_obj_t;
                    if refreshing {
                        lv_obj_remove_flag(s, LV_OBJ_FLAG_HIDDEN);
                    } else {
                        lv_obj_add_flag(s, LV_OBJ_FLAG_HIDDEN);
                    }
                }
                return;
            }
        }
    }
}

pub(in crate::system::picodroid::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        for entry in &mut HANDLE_MAP[..HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if HANDLE_MAP_LEN < MAX_LISTENERS {
            HANDLE_MAP[HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            HANDLE_MAP_LEN += 1;
        }
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_refresh_queue() -> Option<usize> {
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
pub fn lookup_refresh_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &HANDLE_MAP[..HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_swipe_refresh_state() {
    unsafe {
        for slot in &mut SLOTS[..] {
            *slot = EMPTY;
        }
        HANDLE_MAP_LEN = 0;
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}

fn register_layout(container: usize, spinner: usize) {
    unsafe {
        for slot in &mut SLOTS[..] {
            if slot.container == 0 {
                *slot = Slot {
                    container,
                    spinner,
                    refreshing: false,
                };
                return;
            }
        }
    }
}
