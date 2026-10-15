// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Switch` (LVGL `lv_switch`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 16;
struct State {
    queue: LocalRing<usize, QUEUE_SIZE>,
    handle_map: PtrMap<MAX_LISTENERS>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        queue: LocalRing::new(0),
        handle_map: PtrMap::new(),
    })
};

const MAX_LISTENERS: usize = 32;

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    STATE.handle_map.remove(obj)
}

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    STATE.queue.push(obj as usize);
}

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let s = lv_switch_create(lifecycle::screen_ptr());
        lv_obj_add_event_cb(
            s,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            s,
            Some(map_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
        s
    };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn is_checked(id: i32) -> bool {
    unsafe { lv_obj_has_state(handle_table::lookup(id), LV_STATE_CHECKED) }
}

pub(in crate::graphics) fn set_checked(id: i32, checked: bool) {
    unsafe {
        let obj = handle_table::lookup(id);
        if checked {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        }
    }
}

pub(in crate::graphics) fn toggle(id: i32) {
    unsafe {
        let obj = handle_table::lookup(id);
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
    }
}

pub(in crate::graphics) fn perform_checked_change(id: i32) {
    unsafe {
        let obj = handle_table::lookup(id);
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
        lv_obj_send_event(obj, LV_EVENT_VALUE_CHANGED, core::ptr::null_mut());
    }
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if let Upsert::Full = STATE.handle_map.upsert(raw_ptr, obj_ref) {
        warn_full("switch-checked");
    }
}

pub fn drain_sw_checked_change_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn lookup_sw_checked_change_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_switch_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
}

/// Visit the Java `Switch` object ref of every switch registered for a
/// checked-change listener so the GC keeps it alive. A `Switch` referenced only
/// by this native map — e.g. a local in an Activity's `onCreate` that the app
/// never stored in a field, and that `addView` keeps alive only natively — would
/// otherwise be swept on the first GC; its `u16` slot is then reused by a later
/// allocation and the next Activity's dispatch resolves a dead ref →
/// `NoSuchMethod`. See `widgets::button::visit_click_listener_roots`.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.handle_map.visit(visit)
}
