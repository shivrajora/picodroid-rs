// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Spinner` (LVGL `lv_dropdown`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
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

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    STATE.handle_map.remove(obj)
}

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    STATE.queue.push(obj as usize);
}

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let dd = lv_dropdown_create(lifecycle::screen_ptr());
        // LVGL creates a dropdown with no options at all (lv_conf.h turns
        // the "Option 1..3" defaults off); an empty list is what an adapter
        // with no items shows, and it is never a null string to draw.
        lv_dropdown_set_options(dd, c"".as_ptr());
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
    let dd = handle_table::lookup(id);
    if dd.is_null() {
        return; // stale handle — mutating a destroyed View is a no-op
    }
    // Any length: a 128-byte buffer here showed twenty "Option NN" items as
    // twelve and a fragment, and nothing past the twelfth could be picked.
    super::text_view::with_cstr(items, |p| unsafe { lv_dropdown_set_options(dd, p) });
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
        match STATE.handle_map.upsert(raw_ptr, obj_ref) {
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
    STATE.queue.pop()
}

pub fn lookup_spinner_obj(handle: usize) -> Option<u16> {
    STATE.handle_map.lookup(handle)
}

pub fn reset_spinner_state() {
    STATE.handle_map.reset();
    STATE.queue.clear();
}
