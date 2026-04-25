//! LVGL impl of `picodroid.widget.Keyboard` plus the auto-show "system
//! keyboard" singleton tapping into `EditText`.
//!
//! Two paths share this module:
//!
//! - **Explicit Keyboard widget**: each `new Keyboard()` from Java calls
//!   [`create`], which produces a fresh `lv_keyboard` parented to the
//!   active screen. Apps own its lifetime, position, and mode. READY
//!   events route to the per-instance Java listener via the ring buffer
//!   below (mirroring Button's click queue).
//! - **System keyboard**: a single shared `lv_keyboard` lazy-created on
//!   the first `EditText` tap (when auto-show is enabled). Reused across
//!   every EditText for the lifetime of the app run. Dismissed by BACK,
//!   the OK key, or `EditText.hideKeyboard()`. Has *no* Java listener —
//!   apps that need done-detection construct an explicit Keyboard.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

// ── READY event ring buffer (per-instance only) ─────────────────────────────

const READY_QUEUE_SIZE: usize = 16;
static mut READY_QUEUE: [usize; READY_QUEUE_SIZE] = [0; READY_QUEUE_SIZE];
static mut READY_QUEUE_HEAD: usize = 0;
static mut READY_QUEUE_TAIL: usize = 0;

const MAX_KEYBOARDS: usize = 4;
static mut KEYBOARD_HANDLE_MAP: [(usize, u16); MAX_KEYBOARDS] = [(0, 0); MAX_KEYBOARDS];
static mut KEYBOARD_HANDLE_MAP_LEN: usize = 0;

unsafe extern "C" fn keyboard_ready_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    unsafe {
        let head = READY_QUEUE_HEAD;
        let next = (head + 1) % READY_QUEUE_SIZE;
        if next != READY_QUEUE_TAIL {
            READY_QUEUE[head] = obj as usize;
            READY_QUEUE_HEAD = next;
        }
    }
}

// ── System keyboard singleton ───────────────────────────────────────────────
//
// Lazy-created on the first show_system_for() call; persists for the rest
// of the app run. The visible-state mirror is tracked in Rust because the
// existing FFI doesn't expose `lv_obj_has_flag` and growing the surface
// for one read isn't worth it.

static mut SYSTEM_KEYBOARD: *mut lv_obj_t = core::ptr::null_mut();
static mut SYSTEM_KEYBOARD_VISIBLE: bool = false;

unsafe extern "C" fn system_keyboard_ready_cb(_e: *mut lv_event_t) {
    // OK key on the system keyboard auto-hides — no Java callback. Apps
    // that want done-detection construct an explicit Keyboard.
    hide_system();
}

unsafe fn ensure_system_keyboard() -> *mut lv_obj_t {
    unsafe {
        if !SYSTEM_KEYBOARD.is_null() {
            return SYSTEM_KEYBOARD;
        }
        let scr = lifecycle::screen_ptr();
        let kb = lv_keyboard_create(scr);
        // Force an explicit size + position so we don't depend on
        // LVGL's default heuristics (which vary by version). On a
        // 240x240 panel this leaves the top 100px for the form.
        lv_obj_set_pos(kb, 0, 100);
        lv_obj_set_size(kb, 240, 140);
        lv_obj_set_style_bg_opa(kb, LV_OPA_COVER, 0);
        // Hidden until the first show_system_for call.
        lv_obj_add_flag(kb, LV_OBJ_FLAG_HIDDEN);
        lv_obj_add_event_cb(
            kb,
            Some(system_keyboard_ready_cb),
            LV_EVENT_READY,
            core::ptr::null_mut(),
        );
        SYSTEM_KEYBOARD = kb;
        kb
    }
}

/// Show the system keyboard, binding it to `ta`. Lazy-creates on the
/// first call. Called from the EditText auto-show trampoline in
/// [`super::edit_text`].
pub fn show_system_for(ta: *mut lv_obj_t) {
    if ta.is_null() {
        return;
    }
    unsafe {
        let kb = ensure_system_keyboard();
        lv_keyboard_set_textarea(kb, ta);
        lv_obj_remove_flag(kb, LV_OBJ_FLAG_HIDDEN);
        SYSTEM_KEYBOARD_VISIBLE = true;
    }
}

/// Hide the system keyboard if currently visible. Returns `true` if a
/// hide actually happened — used by the BACK-key intercept in
/// `lifecycle.rs::dispatch_key_events` to decide whether to consume the
/// event.
pub fn hide_system() -> bool {
    unsafe {
        if SYSTEM_KEYBOARD.is_null() || !SYSTEM_KEYBOARD_VISIBLE {
            return false;
        }
        lv_obj_add_flag(SYSTEM_KEYBOARD, LV_OBJ_FLAG_HIDDEN);
        SYSTEM_KEYBOARD_VISIBLE = false;
        true
    }
}

// ── Per-instance widget ops (called from widgets/keyboard.rs Java shim) ─────

/// `Keyboard.nativeCreate()` — fresh per-instance keyboard parented to
/// the screen. Distinct from the system keyboard above.
pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let kb = unsafe { lv_keyboard_create(lifecycle::screen_ptr()) };
    unsafe {
        lv_obj_add_event_cb(
            kb,
            Some(keyboard_ready_cb),
            LV_EVENT_READY,
            core::ptr::null_mut(),
        );
    }
    handle_table::register(kb)
}

pub(in crate::system::picodroid::graphics) fn set_textarea(kb_id: i32, ta_id: i32) {
    let kb = handle_table::lookup(kb_id);
    let ta = handle_table::lookup(ta_id);
    if kb.is_null() || ta.is_null() {
        return;
    }
    unsafe { lv_keyboard_set_textarea(kb, ta) };
}

pub(in crate::system::picodroid::graphics) fn set_mode(kb_id: i32, mode: lv_keyboard_mode_t) {
    let kb = handle_table::lookup(kb_id);
    if kb.is_null() {
        return;
    }
    unsafe { lv_keyboard_set_mode(kb, mode) };
}

/// Register a Java `Keyboard` object as the READY-listener target for
/// the given instance. Mirrors the Button pattern.
pub(in crate::system::picodroid::graphics) fn register_ready_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        for entry in &mut KEYBOARD_HANDLE_MAP[..KEYBOARD_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if KEYBOARD_HANDLE_MAP_LEN < MAX_KEYBOARDS {
            KEYBOARD_HANDLE_MAP[KEYBOARD_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            KEYBOARD_HANDLE_MAP_LEN += 1;
        }
    }
}

/// Drain one READY event (raw `lv_obj_t*` value) from the per-instance
/// queue. Returns `None` when empty.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_ready_queue() -> Option<usize> {
    unsafe {
        if READY_QUEUE_TAIL == READY_QUEUE_HEAD {
            return None;
        }
        let h = READY_QUEUE[READY_QUEUE_TAIL];
        READY_QUEUE_TAIL = (READY_QUEUE_TAIL + 1) % READY_QUEUE_SIZE;
        Some(h)
    }
}

/// Look up the Java `Keyboard` object index for a per-instance widget.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_keyboard_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &KEYBOARD_HANDLE_MAP[..KEYBOARD_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_keyboard_state() {
    unsafe {
        KEYBOARD_HANDLE_MAP_LEN = 0;
        READY_QUEUE_HEAD = 0;
        READY_QUEUE_TAIL = 0;
        // The screen tree is torn down by handle_table::reset on app
        // reload, so the system keyboard pointer is dangling — drop our
        // cache so the next show recreates from scratch.
        SYSTEM_KEYBOARD = core::ptr::null_mut();
        SYSTEM_KEYBOARD_VISIBLE = false;
    }
}
