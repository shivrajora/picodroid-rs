//! LVGL impl of `DatePicker` (LVGL `lv_calendar`).
//!
//! Tapping a day cell fires `LV_EVENT_VALUE_CHANGED` on the calendar. The
//! trampoline pushes the calendar pointer onto a ring buffer drained by
//! the framework loop, which calls `DatePicker.fireDateChanged()` on the
//! matching Java object. The selection itself is read on demand via
//! `lv_calendar_get_pressed_date`.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

const QUEUE_SIZE: usize = 8;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

const MAX_LISTENERS: usize = 4;
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
        let cal = lv_calendar_create(lifecycle::screen_ptr());
        lv_obj_add_event_cb(
            cal,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        cal
    };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn set_date(id: i32, year: i32, month: i32, day: i32) {
    let cal = handle_table::lookup(id);
    if cal.is_null() {
        return;
    }
    let y = year.max(0) as u32;
    let m = month.clamp(1, 12) as u32;
    let d = day.clamp(1, 31) as u32;
    unsafe {
        lv_calendar_set_today_date(cal, y, m, d);
        lv_calendar_set_month_shown(cal, y, m);
    }
}

/// Read the most-recently-pressed date. Returns `(year, month, day)` —
/// all zeros if no day has been tapped yet (LVGL returns `LV_RESULT_INVALID`).
pub(in crate::system::picodroid::graphics) fn get_date(id: i32) -> (i32, i32, i32) {
    let cal = handle_table::lookup(id);
    if cal.is_null() {
        return (0, 0, 0);
    }
    let mut date = lv_calendar_date_t::default();
    let res = unsafe { lv_calendar_get_pressed_date(cal, &mut date as *mut _) };
    if res == LV_RESULT_OK {
        (date.year as i32, date.month as i32, date.day as i32)
    } else {
        (0, 0, 0)
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
pub fn drain_date_picker_queue() -> Option<usize> {
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
pub fn lookup_date_picker_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &HANDLE_MAP[..HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_date_picker_state() {
    unsafe {
        HANDLE_MAP_LEN = 0;
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}
