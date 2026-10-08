// SPDX-License-Identifier: GPL-3.0-only
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
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 8;
// SAFETY: filled by LVGL callbacks and drained by natives, all on JVM tasks.
static QUEUE: Core0<LocalRing<usize, QUEUE_SIZE>> = unsafe { Core0::new(LocalRing::new(0)) };

const MAX_LISTENERS: usize = 4;
// SAFETY: a listener registry, reached only from JVM tasks.
static HANDLE_MAP: Core0<PtrMap<MAX_LISTENERS>> = unsafe { Core0::new(PtrMap::new()) };

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    HANDLE_MAP.remove(obj)
}

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    // lv_calendar's inner btnmatrix bubbles VALUE_CHANGED up via
    // LV_OBJ_FLAG_EVENT_BUBBLE (lv_calendar.c:358), so the original
    // event target is the btnmatrix, not the calendar root we registered
    // in HANDLE_MAP. Use the *current* target — the widget this handler
    // is bound to — to recover the calendar pointer.
    let obj = unsafe { lv_event_get_current_target_obj(e) };
    QUEUE.push(obj as usize);
}

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let cal = lv_calendar_create(lifecycle::screen_ptr());
        lv_obj_add_event_cb(
            cal,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        // Both the calendar and its day matrix are group-default, which
        // would make a DatePicker two focus stops on a key board. The matrix
        // is the one the keys edit (keypad.rs::edit_kind_of); the container
        // leaves the ring. A no-op without a group (a touch board).
        lv_group_remove_obj(cal);
        cal
    };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn set_date(id: i32, year: i32, month: i32, day: i32) {
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
pub(in crate::graphics) fn get_date(id: i32) -> (i32, i32, i32) {
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

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        match HANDLE_MAP.upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("date-picker"),
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

pub fn drain_date_picker_queue() -> Option<usize> {
    QUEUE.pop()
}

pub fn lookup_date_picker_obj(handle: usize) -> Option<u16> {
    HANDLE_MAP.lookup(handle)
}

pub fn reset_date_picker_state() {
    HANDLE_MAP.reset();
    QUEUE.clear();
}
