// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Spinner` (LVGL `lv_dropdown`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 16;
struct State {
    /// Selection changes, each the dropdown's handle.
    queue: LocalRing<i32, QUEUE_SIZE>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        queue: LocalRing::new(0),
    })
};

lv_event_cb! {
    fn value_changed_cb(e) {
        let handle = handle_table::handle_of(e.target());
        if handle != 0 {
            STATE.queue.push(handle);
        }
    }
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(dd) = lifecycle::screen_ptr().and_then(|o| lv_dropdown_create(o)) else {
        return 0;
    };
    // LVGL creates a dropdown with no options at all (lv_conf.h turns
    // the "Option 1..3" defaults off); an empty list is what an adapter
    // with no items shows, and it is never a null string to draw.
    // SAFETY: `dd` was just made by `lv_dropdown_create`; the options are a
    // NUL-terminated literal LVGL copies.
    unsafe { lv_dropdown_set_options(dd, c"".as_ptr()) };
    add_event_cb(dd, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    handle_table::register(dd)
}

pub(in crate::graphics) fn set_items(id: i32, items: &str) {
    let Some(dd) = handle_table::get(id) else {
        return; // stale handle — mutating a destroyed View is a no-op
    };
    // Any length: a 128-byte buffer here showed twenty "Option NN" items as
    // twelve and a fragment, and nothing past the twelfth could be picked.
    // SAFETY: a `Spinner` native: its constructors give `AdapterView` the
    // `lv_dropdown` [`create`] made, and no other handle.
    // The options are NUL-terminated and LVGL copies them.
    super::text_view::with_cstr(items, |p| unsafe {
        lv_dropdown_set_options(dd, p.as_ptr())
    });
}

/// The selected row; 0 for a stale handle.
pub(in crate::graphics) fn get_selected(id: i32) -> i32 {
    let Some(dd) = handle_table::get(id) else {
        return 0;
    };
    // SAFETY: a `Spinner` native: its constructors give `AdapterView` the
    // `lv_dropdown` [`create`] made, and no other handle.
    unsafe { lv_dropdown_get_selected(dd) as i32 }
}

pub(in crate::graphics) fn perform_item_selected(id: i32) {
    if let Some(dd) = handle_table::get(id) {
        send_event(dd, Notice::ValueChanged);
    }
}

pub fn drain_spinner_change_queue() -> Option<i32> {
    STATE.queue.pop()
}

pub fn reset_spinner_state() {
    STATE.queue.clear();
}
