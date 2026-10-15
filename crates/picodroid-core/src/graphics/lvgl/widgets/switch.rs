// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Switch` (LVGL `lv_switch`).

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
    let Some(s) = lifecycle::screen_ptr().and_then(|o| lv_switch_create(o)) else {
        return 0;
    };
    add_event_cb(s, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    add_event_cb(s, map_delete_cb, LV_EVENT_DELETE);
    handle_table::register(s)
}

pub(in crate::graphics) fn is_checked(id: i32) -> bool {
    super::compound::is_checked(id)
}

pub(in crate::graphics) fn set_checked(id: i32, checked: bool) {
    super::compound::set_checked(id, checked);
}

pub(in crate::graphics) fn toggle(id: i32) {
    super::compound::toggle(id);
}

pub(in crate::graphics) fn perform_checked_change(id: i32) {
    super::compound::perform_checked_change(id);
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    super::compound::register_listener(&STATE.handle_map, id, obj_ref, "switch-checked");
}

pub fn drain_sw_checked_change_queue() -> Option<usize> {
    STATE.queue.pop()
}

pub fn lookup_sw_checked_change_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_switch_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
}

/// Visit the Java `Switch` object ref of every switch registered for a
/// checked-change listener so the GC keeps it alive. A `Switch` referenced only
/// by this native map — e.g. a local in an Activity's `onCreate` that the app
/// never stored in a field, and that `addView` keeps alive only natively — would
/// otherwise be swept on the first GC; its `u16` slot is then reused by a later
/// allocation and the next Activity's dispatch resolves a dead ref →
/// `NoSuchMethod`. See `widgets::button::visit_click_listener_roots`.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.handle_map.visit(visit)
}
