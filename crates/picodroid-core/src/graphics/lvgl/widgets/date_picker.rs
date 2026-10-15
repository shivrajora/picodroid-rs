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
use super::super::listener_map::PtrMap;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 8;
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

const MAX_LISTENERS: usize = 4;

lv_event_cb! {
    fn map_delete_cb(e) {
        let obj = e.target().addr();
        STATE.handle_map.remove(obj)
    }
}

lv_event_cb! {
    fn value_changed_cb(e) {
        // lv_calendar's inner btnmatrix bubbles VALUE_CHANGED up via
        // LV_OBJ_FLAG_EVENT_BUBBLE (lv_calendar.c:358), so the original
        // event target is the btnmatrix, not the calendar root we registered
        // in STATE.handle_map. Use the *current* target — the widget this
        // handler is bound to — to recover the calendar pointer.
        STATE.queue.push(e.current_target().addr());
    }
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(cal) = lifecycle::screen_ptr().and_then(|o| lv_calendar_create(o)) else {
        return 0;
    };
    add_event_cb(cal, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    // Both the calendar and its day matrix are group-default, which
    // would make a DatePicker two focus stops on a key board. The matrix
    // is the one the keys edit (keypad.rs::edit_kind_of); the container
    // leaves the ring. A no-op without a group (a touch board).
    lv_group_remove_obj(cal);
    handle_table::register(cal)
}

pub(in crate::graphics) fn set_date(id: i32, year: i32, month: i32, day: i32) {
    let Some(cal) = handle_table::get(id) else {
        return;
    };
    let y = year.max(0) as u32;
    let m = month.clamp(1, 12) as u32;
    let d = day.clamp(1, 31) as u32;
    // SAFETY: a `DatePicker` native: its constructor gives `View` the
    // `lv_calendar` [`create`] made, and no other handle.
    unsafe {
        lv_calendar_set_today_date(cal, y, m, d);
        lv_calendar_set_month_shown(cal, y, m);
    }
}

/// Read the most-recently-pressed date. Returns `(year, month, day)` —
/// all zeros if no day has been tapped yet (LVGL returns `LV_RESULT_INVALID`).
pub(in crate::graphics) fn get_date(id: i32) -> (i32, i32, i32) {
    let Some(cal) = handle_table::get(id) else {
        return (0, 0, 0);
    };
    let mut date = lv_calendar_date_t::default();
    // SAFETY: a `DatePicker` native: its constructor gives `View` the
    // `lv_calendar` [`create`] made, and no other handle.
    let res = unsafe { lv_calendar_get_pressed_date(cal, &mut date) };
    if res == LV_RESULT_OK {
        (date.year as i32, date.month as i32, date.day as i32)
    } else {
        (0, 0, 0)
    }
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    super::compound::register_listener_with_delete_hook(
        &STATE.handle_map,
        id,
        obj_ref,
        map_delete_cb,
        "date-picker",
    );
}

pub fn drain_date_picker_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn lookup_date_picker_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_date_picker_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
}
