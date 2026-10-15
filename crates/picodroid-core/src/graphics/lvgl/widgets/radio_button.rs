// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `RadioButton` — an `lv_checkbox` whose indicator is styled
//! circular (LV_RADIUS_CIRCLE on LV_PART_INDICATOR), the standard LVGL
//! recipe for radio appearance. Mutual exclusion lives in Java
//! (`RadioGroup`); this module is a plain two-state widget, cloned from
//! `check_box.rs`.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::PtrMap;
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

lv_event_cb! {
    fn map_delete_cb(e) {
        let obj = e.target().addr();
        STATE.handle_map.remove(obj)
    }
}

lv_event_cb! {
    fn value_changed_cb(e) {
        let obj = e.target();
        STATE.queue.push(obj.addr());
    }
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(rb) = lifecycle::screen_ptr().and_then(|o| lv_checkbox_create(o)) else {
        return 0;
    };
    lv_obj_set_style_radius(rb, LV_RADIUS_CIRCLE, LV_PART_INDICATOR);
    add_event_cb(rb, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    add_event_cb(rb, map_delete_cb, LV_EVENT_DELETE);
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

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    super::compound::register_listener(&STATE.handle_map, id, obj_ref, "radio-checked");
}

pub fn drain_rb_checked_change_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn lookup_rb_checked_change_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_radio_button_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
}

/// GC roots for the radio listener map — same unrooted-View hazard as
/// `check_box::visit_checked_change_listener_roots`. Every RadioButton in
/// a RadioGroup is registered here (the group wires its internal listener),
/// so grouped radios survive GC even when the app keeps no Java field.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.handle_map.visit(visit)
}
