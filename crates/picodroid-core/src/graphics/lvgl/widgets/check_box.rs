// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `CheckBox` (LVGL `lv_checkbox`).

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
        let cb = lv_checkbox_create(lifecycle::screen_ptr());
        lv_obj_add_event_cb(
            cb,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            cb,
            Some(map_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
        cb
    };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn set_text(id: i32, text: &str) {
    let cb = handle_table::lookup(id);
    if cb.is_null() {
        return; // stale handle — mutating a destroyed View is a no-op
    }
    super::text_view::with_cstr(text, |p| unsafe { lv_checkbox_set_text(cb, p) });
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
        warn_full("checkbox-checked");
    }
}

pub fn drain_cb_checked_change_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn lookup_cb_checked_change_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_check_box_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
}

/// Visit the Java `CheckBox` object ref of every checkbox registered for a
/// checked-change listener so the GC keeps it alive. A `CheckBox` referenced
/// only by this native map (no Java field; `addView` keeps it alive only
/// natively) would otherwise be swept on the first GC, its slot reused, and a
/// later dispatch resolves a dead ref → `NoSuchMethod`. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.handle_map.visit(visit)
}
