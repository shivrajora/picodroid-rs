// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Spinner` (LVGL `lv_dropdown`).

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
        let dd = lv_dropdown_create(lifecycle::screen_ptr());
        lv_obj_add_event_cb(
            dd,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        dd
    };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn set_items(id: i32, items: &str) {
    let mut buf = [0u8; 128];
    let len = items.len().min(127);
    buf[..len].copy_from_slice(&items.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_dropdown_set_options(handle_table::lookup(id), buf.as_ptr() as *const c_char) };
}

pub(in crate::graphics) fn get_selected(id: i32) -> i32 {
    unsafe { lv_dropdown_get_selected(handle_table::lookup(id)) as i32 }
}

pub(in crate::graphics) fn perform_item_selected(id: i32) {
    unsafe {
        lv_obj_send_event(
            handle_table::lookup(id),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
    }
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        match map_mut(&raw mut HANDLE_MAP).upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("spinner"),
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

pub fn drain_spinner_change_queue() -> Option<usize> {
    unsafe {
        if QUEUE_TAIL == QUEUE_HEAD {
            return None;
        }
        let h = QUEUE[QUEUE_TAIL];
        QUEUE_TAIL = (QUEUE_TAIL + 1) % QUEUE_SIZE;
        Some(h)
    }
}

pub fn lookup_spinner_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const HANDLE_MAP).lookup(handle) }
}

pub fn reset_spinner_state() {
    unsafe {
        map_mut(&raw mut HANDLE_MAP).reset();
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}
