// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `RadioButton` — an `lv_checkbox` whose indicator is styled
//! circular (LV_RADIUS_CIRCLE on LV_PART_INDICATOR), the standard LVGL
//! recipe for radio appearance. Mutual exclusion lives in Java
//! (`RadioGroup`); this module is a plain two-state widget, cloned from
//! `check_box.rs`.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const QUEUE_SIZE: usize = 16;
struct State {
    /// Checked-change events, each the widget's handle; the drain resolves
    /// the Java object through the handle table's binding, so a widget
    /// nobody set a listener on resolves to nothing.
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
    let Some(rb) = lifecycle::screen_ptr().and_then(|o| lv_checkbox_create(o)) else {
        return 0;
    };
    lv_obj_set_style_radius(rb, LV_RADIUS_CIRCLE, LV_PART_INDICATOR);
    add_event_cb(rb, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    handle_table::register(rb)
}

pub(in crate::graphics) fn set_text(id: i32, text: &str) {
    let Some(rb) = handle_table::get(id) else {
        return; // stale handle — mutating a destroyed View is a no-op
    };
    // SAFETY: `setText` is a native of RadioButton, whose constructors give it
    // the `lv_checkbox` [`create`] made and no other handle; the text is
    // NUL-terminated and LVGL copies it.
    super::text_view::with_cstr(text, |p| unsafe { lv_checkbox_set_text(rb, p.as_ptr()) });
}

pub(in crate::graphics) fn is_checked(id: i32) -> bool {
    super::compound::is_checked(id)
}

pub(in crate::graphics) fn set_checked(id: i32, checked: bool) {
    super::compound::set_checked(id, checked);
}

pub(in crate::graphics) fn perform_checked_change(id: i32) {
    super::compound::perform_checked_change(id);
}

pub fn drain_rb_checked_change_queue() -> Option<i32> {
    STATE.queue.pop()
}

pub fn reset_radio_button_state() {
    STATE.queue.clear();
}
