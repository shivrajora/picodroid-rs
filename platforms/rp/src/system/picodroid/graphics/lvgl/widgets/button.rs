// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of the click pathway shared by all View subclasses.
//!
//! `Button::create()` constructs an `lv_button` + child `lv_label` for the
//! Button widget specifically. The click event plumbing — ring buffer,
//! handle→ObjectRef map, `LV_EVENT_CLICKED` trampoline, `performClick` —
//! lives here but is reached through `View.setOnClickListener` /
//! `View.performClick`, so any view (TextView, layout, ImageView) that
//! attaches a click listener becomes clickable. The LVGL CLICKABLE flag
//! and the event callback are both attached lazily on first listener
//! registration to avoid emitting events on widgets that don't care.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

// ── Click event queue (ring buffer) ─────────────────────────────────────────

const CLICK_QUEUE_SIZE: usize = 16;
static mut CLICK_QUEUE: [usize; CLICK_QUEUE_SIZE] = [0; CLICK_QUEUE_SIZE];
static mut CLICK_QUEUE_HEAD: usize = 0;
static mut CLICK_QUEUE_TAIL: usize = 0;

// ── Handle → Java object mapping (for click dispatch) ───────────────────────

const MAX_CLICK_VIEWS: usize = 32;
static mut VIEW_CLICK_MAP: [(usize, u16); MAX_CLICK_VIEWS] = [(0, 0); MAX_CLICK_VIEWS];
static mut VIEW_CLICK_MAP_LEN: usize = 0;

// ── LVGL trampoline ─────────────────────────────────────────────────────────

unsafe extern "C" fn view_click_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    unsafe {
        let head = CLICK_QUEUE_HEAD;
        let next = (head + 1) % CLICK_QUEUE_SIZE;
        if next != CLICK_QUEUE_TAIL {
            CLICK_QUEUE[head] = obj as usize;
            CLICK_QUEUE_HEAD = next;
        }
    }
}

// ── LVGL ops (plain-Rust; called from widgets/*.rs Java shims) ──────────────

/// Create `lv_button` with a centered child label set to `text`. Returns
/// the Java-side `nativeHandle`. Click events are not wired here — the
/// `LV_EVENT_CLICKED` trampoline is attached lazily by [`register_click_listener`]
/// on the first `View.setOnClickListener` call.
pub(in crate::system::picodroid::graphics) fn create(text: &str) -> i32 {
    let btn = unsafe { lv_button_create(lifecycle::screen_ptr()) };
    let label = unsafe { lv_label_create(btn) };

    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        lv_label_set_text(label, buf.as_ptr() as *const c_char);
        lv_obj_center(label);
    }

    handle_table::register(btn)
}

/// Set the text on the button's child label.
pub(in crate::system::picodroid::graphics) fn set_text(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        let label = lv_obj_get_child(handle_table::lookup(id), 0);
        if !label.is_null() {
            lv_label_set_text(label, buf.as_ptr() as *const c_char);
        }
    }
}

/// Synthetically fire `LV_EVENT_CLICKED` on the underlying widget.
/// No-op if no click listener has been registered (no trampoline attached).
pub(in crate::system::picodroid::graphics) fn perform_click(id: i32) {
    unsafe {
        lv_obj_send_event(
            handle_table::lookup(id),
            LV_EVENT_CLICKED,
            core::ptr::null_mut(),
        );
    }
}

/// Register a Java `View` object as the click-listener target for the
/// given handle. On the first registration for a widget, attaches the
/// `LV_EVENT_CLICKED` trampoline and sets `LV_OBJ_FLAG_CLICKABLE` so any
/// view (not just Button) becomes clickable.
pub(in crate::system::picodroid::graphics) fn register_click_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        for entry in &mut VIEW_CLICK_MAP[..VIEW_CLICK_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if VIEW_CLICK_MAP_LEN < MAX_CLICK_VIEWS {
            VIEW_CLICK_MAP[VIEW_CLICK_MAP_LEN] = (raw_ptr, obj_ref);
            VIEW_CLICK_MAP_LEN += 1;
            // First registration for this widget — wire LVGL plumbing.
            let obj = raw_ptr as *mut lv_obj_t;
            lv_obj_add_flag(obj, LV_OBJ_FLAG_CLICKABLE);
            lv_obj_add_event_cb(
                obj,
                Some(view_click_cb),
                LV_EVENT_CLICKED,
                core::ptr::null_mut(),
            );
        }
    }
}

/// Drain one click event (raw `lv_obj_t*` value) from the queue.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_click_queue() -> Option<usize> {
    unsafe {
        if CLICK_QUEUE_TAIL == CLICK_QUEUE_HEAD {
            return None;
        }
        let handle = CLICK_QUEUE[CLICK_QUEUE_TAIL];
        CLICK_QUEUE_TAIL = (CLICK_QUEUE_TAIL + 1) % CLICK_QUEUE_SIZE;
        Some(handle)
    }
}

/// Look up the Java `View` object index for a clickable widget's raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_button_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &VIEW_CLICK_MAP[..VIEW_CLICK_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_button_state() {
    unsafe {
        VIEW_CLICK_MAP_LEN = 0;
        CLICK_QUEUE_HEAD = 0;
        CLICK_QUEUE_TAIL = 0;
    }
}
