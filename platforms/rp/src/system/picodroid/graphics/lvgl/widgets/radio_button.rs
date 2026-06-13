// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `RadioButton` — an `lv_checkbox` whose indicator is styled
//! circular (LV_RADIUS_CIRCLE on LV_PART_INDICATOR), the standard LVGL
//! recipe for radio appearance. Mutual exclusion lives in Java
//! (`RadioGroup`); this module is a plain two-state widget, cloned from
//! `check_box.rs`.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

const QUEUE_SIZE: usize = 16;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

const MAX_LISTENERS: usize = 32;
static mut HANDLE_MAP: [(usize, u16); MAX_LISTENERS] = [(0, 0); MAX_LISTENERS];
static mut HANDLE_MAP_LEN: usize = 0;

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

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let rb = lv_checkbox_create(lifecycle::screen_ptr());
        lv_obj_set_style_radius(rb, LV_RADIUS_CIRCLE, LV_PART_INDICATOR);
        lv_obj_add_event_cb(
            rb,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        rb
    };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn set_text(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        lv_checkbox_set_text(handle_table::lookup(id), buf.as_ptr() as *const c_char);
    }
}

pub(in crate::system::picodroid::graphics) fn is_checked(id: i32) -> bool {
    unsafe { lv_obj_has_state(handle_table::lookup(id), LV_STATE_CHECKED) }
}

pub(in crate::system::picodroid::graphics) fn set_checked(id: i32, checked: bool) {
    unsafe {
        let obj = handle_table::lookup(id);
        if checked {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        }
    }
}

pub(in crate::system::picodroid::graphics) fn perform_checked_change(id: i32) {
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

pub(in crate::system::picodroid::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
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
pub fn drain_rb_checked_change_queue() -> Option<usize> {
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
pub fn lookup_rb_checked_change_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &HANDLE_MAP[..HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_radio_button_state() {
    unsafe {
        HANDLE_MAP_LEN = 0;
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}

/// GC roots for the radio listener map — same unrooted-View hazard as
/// `check_box::visit_checked_change_listener_roots`. Every RadioButton in
/// a RadioGroup is registered here (the group wires its internal listener),
/// so grouped radios survive GC even when the app keeps no Java field.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    unsafe {
        for &(_, r) in &HANDLE_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
    }
}
