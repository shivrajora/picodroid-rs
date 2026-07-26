// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `CheckBox` (LVGL `lv_checkbox`).

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{map_mut, map_ref, warn_full, PtrMap, Upsert};

const QUEUE_SIZE: usize = 16;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

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
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        lv_checkbox_set_text(handle_table::lookup(id), buf.as_ptr() as *const c_char);
    }
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
    unsafe {
        if let Upsert::Full = map_mut(&raw mut HANDLE_MAP).upsert(raw_ptr, obj_ref) {
            warn_full("checkbox-checked");
        }
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_cb_checked_change_queue() -> Option<usize> {
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
pub fn lookup_cb_checked_change_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const HANDLE_MAP).lookup(handle) }
}

pub fn reset_check_box_state() {
    unsafe {
        map_mut(&raw mut HANDLE_MAP).reset();
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}

/// Visit the Java `CheckBox` object ref of every checkbox registered for a
/// checked-change listener so the GC keeps it alive. A `CheckBox` referenced
/// only by this native map (no Java field; `addView` keeps it alive only
/// natively) would otherwise be swept on the first GC, its slot reused, and a
/// later dispatch resolves a dead ref → `NoSuchMethod`. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    unsafe { map_ref(&raw const HANDLE_MAP).visit(visit) }
}
