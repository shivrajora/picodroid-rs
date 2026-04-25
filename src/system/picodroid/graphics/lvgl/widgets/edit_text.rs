//! LVGL impl of `EditText` (LVGL `lv_textarea`).
//!
//! Auto-show keyboard wiring lives here too: every textarea created
//! through this module gets an `LV_EVENT_PRESSED` trampoline registered
//! that — gated by the per-handle [`AUTOSHOW_DISABLED`] map — calls
//! [`super::keyboard::show_system_for`] when the user taps it.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;
use super::keyboard;

// ── Auto-show opt-out registry ──────────────────────────────────────────────
//
// Default is "auto-show enabled" — we track *opt-outs* (the negative
// sense) so a freshly-created EditText behaves correctly without any
// register call from the Java side. Only EditTexts that explicitly call
// `setShowKeyboardOnTouch(false)` end up in this list.

const MAX_AUTOSHOW_OPTOUTS: usize = 16;
static mut AUTOSHOW_DISABLED: [usize; MAX_AUTOSHOW_OPTOUTS] = [0; MAX_AUTOSHOW_OPTOUTS];
static mut AUTOSHOW_DISABLED_LEN: usize = 0;

fn is_autoshow_disabled(raw_ptr: usize) -> bool {
    unsafe {
        for entry in &AUTOSHOW_DISABLED[..AUTOSHOW_DISABLED_LEN] {
            if *entry == raw_ptr {
                return true;
            }
        }
    }
    false
}

unsafe extern "C" fn textarea_pressed_cb(e: *mut lv_event_t) {
    let ta = unsafe { lv_event_get_target_obj(e) };
    if ta.is_null() {
        return;
    }
    if is_autoshow_disabled(ta as usize) {
        return;
    }
    keyboard::show_system_for(ta);
}

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_textarea_create(lifecycle::screen_ptr()) };
    unsafe {
        lv_obj_add_event_cb(
            ptr,
            Some(textarea_pressed_cb),
            LV_EVENT_PRESSED,
            core::ptr::null_mut(),
        );
    }
    handle_table::register(ptr)
}

/// Toggle the auto-show-keyboard-on-touch behavior for `id`. Default
/// after `create` is "enabled"; calling `set_autoshow(id, false)` adds
/// the textarea to the opt-out registry, and `set_autoshow(id, true)`
/// removes it. Idempotent in both directions.
pub(in crate::system::picodroid::graphics) fn set_autoshow(id: i32, enabled: bool) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        if enabled {
            // Remove from opt-out list. Compact in place.
            let mut i = 0;
            while i < AUTOSHOW_DISABLED_LEN {
                if AUTOSHOW_DISABLED[i] == raw_ptr {
                    AUTOSHOW_DISABLED[i] = AUTOSHOW_DISABLED[AUTOSHOW_DISABLED_LEN - 1];
                    AUTOSHOW_DISABLED_LEN -= 1;
                    return;
                }
                i += 1;
            }
        } else {
            // Add to opt-out list (no-op if already present).
            for entry in &AUTOSHOW_DISABLED[..AUTOSHOW_DISABLED_LEN] {
                if *entry == raw_ptr {
                    return;
                }
            }
            if AUTOSHOW_DISABLED_LEN < MAX_AUTOSHOW_OPTOUTS {
                AUTOSHOW_DISABLED[AUTOSHOW_DISABLED_LEN] = raw_ptr;
                AUTOSHOW_DISABLED_LEN += 1;
            }
        }
    }
}

pub fn reset_edit_text_state() {
    unsafe {
        AUTOSHOW_DISABLED_LEN = 0;
    }
}

pub(in crate::system::picodroid::graphics) fn set_text(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_textarea_set_text(handle_table::lookup(id), buf.as_ptr() as *const c_char) };
}

pub(in crate::system::picodroid::graphics) fn set_hint(id: i32, hint: &str) {
    let mut buf = [0u8; 128];
    let len = hint.len().min(127);
    buf[..len].copy_from_slice(&hint.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        lv_textarea_set_placeholder_text(handle_table::lookup(id), buf.as_ptr() as *const c_char)
    };
}

/// Read the current textarea content into `dst` (capped at 256 bytes).
/// Returns the byte length written, or `None` if the textarea is empty
/// or LVGL returned a null pointer.
pub(in crate::system::picodroid::graphics) fn get_text(
    id: i32,
    dst: &mut [u8; 256],
) -> Option<usize> {
    let cstr = unsafe { lv_textarea_get_text(handle_table::lookup(id)) };
    if cstr.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *cstr.add(len) != 0 && len < dst.len() {
            len += 1;
        }
    }
    for (i, slot) in dst[..len].iter_mut().enumerate() {
        *slot = unsafe { *cstr.add(i) } as u8;
    }
    Some(len)
}
