//! LVGL impl of `Button` (LVGL `lv_button` + child `lv_label`).
//!
//! Click events feed a static ring buffer drained by the framework event
//! loop. The handle→Java-ObjectRef map keys on the raw `lv_obj_t*`
//! address (matches the value stored by the LVGL trampoline).

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

const MAX_BUTTONS: usize = 32;
static mut BUTTON_HANDLE_MAP: [(usize, u16); MAX_BUTTONS] = [(0, 0); MAX_BUTTONS];
static mut BUTTON_HANDLE_MAP_LEN: usize = 0;

// ── LVGL trampoline ─────────────────────────────────────────────────────────

unsafe extern "C" fn button_click_cb(e: *mut lv_event_t) {
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

// ── LVGL ops (plain-Rust; called from widgets/button.rs Java shim) ──────────

/// Create `lv_button` with a centered child label set to `text`. Returns
/// the Java-side `nativeHandle`.
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
        lv_obj_add_event_cb(
            btn,
            Some(button_click_cb),
            LV_EVENT_CLICKED,
            core::ptr::null_mut(),
        );
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

/// Drain a click event for `id` if any are queued; returns `true` when
/// consumed.
pub(in crate::system::picodroid::graphics) fn was_clicked(id: i32) -> bool {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        let mut i = CLICK_QUEUE_TAIL;
        while i != CLICK_QUEUE_HEAD {
            if CLICK_QUEUE[i] == raw_ptr {
                // Compact the queue: shift remaining entries down by one.
                let mut j = i;
                loop {
                    let next = (j + 1) % CLICK_QUEUE_SIZE;
                    if next == CLICK_QUEUE_HEAD {
                        break;
                    }
                    CLICK_QUEUE[j] = CLICK_QUEUE[next];
                    j = next;
                }
                CLICK_QUEUE_HEAD = if CLICK_QUEUE_HEAD == 0 {
                    CLICK_QUEUE_SIZE - 1
                } else {
                    CLICK_QUEUE_HEAD - 1
                };
                return true;
            }
            i = (i + 1) % CLICK_QUEUE_SIZE;
        }
    }
    false
}

/// Synthetically fire `LV_EVENT_CLICKED` on the underlying button.
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
/// given handle.
pub(in crate::system::picodroid::graphics) fn register_click_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        for entry in &mut BUTTON_HANDLE_MAP[..BUTTON_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if BUTTON_HANDLE_MAP_LEN < MAX_BUTTONS {
            BUTTON_HANDLE_MAP[BUTTON_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            BUTTON_HANDLE_MAP_LEN += 1;
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

/// Look up the Java `View` object index for a button's raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_button_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &BUTTON_HANDLE_MAP[..BUTTON_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_button_state() {
    unsafe {
        BUTTON_HANDLE_MAP_LEN = 0;
        CLICK_QUEUE_HEAD = 0;
        CLICK_QUEUE_TAIL = 0;
    }
}
