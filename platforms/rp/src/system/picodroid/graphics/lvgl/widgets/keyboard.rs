// SPDX-License-Identifier: GPL-3.0-only
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
//!   the OK key, a tap outside the keyboard, or `EditText.hideKeyboard()`.
//!   Slides up from the screen edge on show; hide is instant.
//!
//! The system keyboard tracks the currently-bound EditText's Java
//! ObjectRef so the OK key can dispatch `EditText.fireEditorAction`
//! through [`drain_editor_action`] without a per-instance Java listener.

use crate::lvgl_ffi::*;

use super::super::animations;
use super::super::events;
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

const SYSTEM_KEYBOARD_REST_Y: i32 = 100;
const SYSTEM_KEYBOARD_OFFSCREEN_Y: i32 = 240;
const SYSTEM_KEYBOARD_SLIDE_DURATION_MS: u32 = 200;

static mut SYSTEM_KEYBOARD: *mut lv_obj_t = core::ptr::null_mut();
static mut SYSTEM_KEYBOARD_HANDLE: i32 = 0;
static mut SYSTEM_KEYBOARD_VISIBLE: bool = false;
/// Java `ObjectRef` of the EditText that triggered the most recent show.
/// Read by [`drain_editor_action`] after the OK key fires.
static mut SYSTEM_KEYBOARD_BOUND_ET: u16 = 0;

/// Pending editor-action record drained from the JVM event pump in
/// `lifecycle::dispatch_editor_actions`. Single-slot: the OK key can only
/// fire once per visibility cycle, and the slot is consumed before the
/// next show.
#[derive(Copy, Clone)]
pub struct EditorActionRecord {
    pub edit_text_ref: u16,
    pub action_id: i32,
}

static mut PENDING_EDITOR_ACTION: Option<EditorActionRecord> = None;

unsafe extern "C" fn system_keyboard_ready_cb(_e: *mut lv_event_t) {
    // OK key on the system keyboard: queue an editor-action for dispatch
    // and dismiss. The Java listener may return `true` to suppress the
    // dismiss — that decision is made by the dispatch site after Java
    // returns, so we hide first and the dispatch site re-shows if needed.
    // For v1 we always hide; suppressing dismiss is a follow-up if any
    // app actually requires it.
    unsafe {
        if SYSTEM_KEYBOARD_BOUND_ET != 0 {
            PENDING_EDITOR_ACTION = Some(EditorActionRecord {
                edit_text_ref: SYSTEM_KEYBOARD_BOUND_ET,
                action_id: 6, // EditorInfo.IME_ACTION_DONE
            });
        }
    }
    hide_system();
}

unsafe extern "C" fn screen_press_outside_cb(e: *mut lv_event_t) {
    let target = unsafe { lv_event_get_target_obj(e) };
    if target.is_null() {
        return;
    }
    let kb = unsafe { SYSTEM_KEYBOARD };
    if kb.is_null() {
        return;
    }
    // Walk parent chain — if we hit the keyboard, this press was on the
    // keyboard itself or one of its keys, so do nothing. Otherwise the
    // tap landed outside and we dismiss.
    let mut cur: *mut lv_obj_t = target;
    while !cur.is_null() {
        if cur == kb {
            return;
        }
        cur = unsafe { lv_obj_get_parent(cur) };
    }
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
        lv_obj_set_pos(kb, 0, SYSTEM_KEYBOARD_REST_Y);
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
        // Register the keyboard in the handle table so `animations::start`
        // can locate it by handle on each tick.
        SYSTEM_KEYBOARD_HANDLE = handle_table::register(kb);
        kb
    }
}

/// Show the system keyboard, binding it to `ta` and recording the Java
/// EditText `obj_ref` for later editor-action dispatch. Lazy-creates the
/// keyboard on the first call. Slides up from the screen edge on a
/// hidden→visible transition; a re-show while already visible re-runs
/// the slide for visual feedback. Called from the EditText auto-show
/// trampoline in [`super::edit_text`].
pub fn show_system_for(ta: *mut lv_obj_t, et_obj_ref: u16) {
    if ta.is_null() {
        return;
    }
    unsafe {
        let kb = ensure_system_keyboard();
        lv_keyboard_set_textarea(kb, ta);
        // Pick the keypad layout for the field being bound. EditTexts flagged
        // numeric (setInputType TYPE_CLASS_NUMBER) get the digit pad; everything
        // else gets the default text layout. Set every show because the system
        // keyboard is shared across fields.
        let mode = if super::edit_text::is_numeric(ta as usize) {
            LV_KEYBOARD_MODE_NUMBER
        } else {
            LV_KEYBOARD_MODE_TEXT_LOWER
        };
        lv_keyboard_set_mode(kb, mode);
        SYSTEM_KEYBOARD_BOUND_ET = et_obj_ref;
        // Visibility flag must be cleared *before* starting the y-anim,
        // otherwise the first frame paints at the off-screen y position
        // while still HIDDEN — fine — and then becomes visible mid-slide
        // which produces a pop. Clearing first means LVGL renders every
        // frame of the slide.
        lv_obj_remove_flag(kb, LV_OBJ_FLAG_HIDDEN);
        lv_obj_set_y(kb, SYSTEM_KEYBOARD_OFFSCREEN_Y);
        animations::start(
            SYSTEM_KEYBOARD_HANDLE,
            /* PROPERTY_Y */ 2,
            SYSTEM_KEYBOARD_OFFSCREEN_Y,
            SYSTEM_KEYBOARD_REST_Y,
            SYSTEM_KEYBOARD_SLIDE_DURATION_MS,
        );
        SYSTEM_KEYBOARD_VISIBLE = true;
        // Attach press-outside dismiss after the EditText's PRESSED event
        // has finished bubbling — the screen-level callback will not
        // receive the same press that opened us.
        events::attach_screen_press_hook(Some(screen_press_outside_cb));
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
        // Cancel any in-flight slide so the keyboard's saved y doesn't
        // creep back to the rest position after the next show. The next
        // show_system_for re-snaps y=offscreen explicitly anyway, so
        // this is belt-and-braces.
        animations::cancel(SYSTEM_KEYBOARD_HANDLE);
        lv_obj_add_flag(SYSTEM_KEYBOARD, LV_OBJ_FLAG_HIDDEN);
        SYSTEM_KEYBOARD_VISIBLE = false;
        SYSTEM_KEYBOARD_BOUND_ET = 0;
        events::detach_screen_press_hook();
        true
    }
}

/// Pop the pending editor-action, if any. The JVM event pump in
/// `lifecycle.rs` consumes this every tick.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_editor_action() -> Option<EditorActionRecord> {
    // Manual take() — `Option::take` would require &mut to a mutable
    // static, which trips Rust 2024's `static_mut_refs` lint. The record
    // is `Copy`, so a load + store is equivalent and lint-clean.
    unsafe {
        let popped = PENDING_EDITOR_ACTION;
        PENDING_EDITOR_ACTION = None;
        popped
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
        SYSTEM_KEYBOARD_HANDLE = 0;
        SYSTEM_KEYBOARD_VISIBLE = false;
        SYSTEM_KEYBOARD_BOUND_ET = 0;
        PENDING_EDITOR_ACTION = None;
    }
    // Same lifetime as the system keyboard — the screen press hook is
    // attached only while the keyboard is visible, so the cached cb
    // pointer must die alongside the screen on reload.
    events::reset_screen_press_hook_state();
}
