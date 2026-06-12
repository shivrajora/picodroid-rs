// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `NumberPicker` (card-styled `lv_obj` + centered `lv_label`).
//!
//! Android's NumberPicker is a touch scroll-wheel; picodroid renders the
//! current value in a focusable card box and steps it from the keypad edit
//! mode (see `super::super::edit_mode`). All value/range/step logic lives in
//! Java — this module only draws the label, queues step requests (+1/-1) for
//! `lifecycle::dispatch_number_picker_steps`, and provides the focus/edit
//! outline visuals the default theme doesn't give plain `lv_obj`s.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

const MAX_PICKERS: usize = 16;
/// (container raw ptr, Java obj_ref). Registered from the `NumberPicker`
/// constructor via `nativeRegisterPicker`, so — unlike the listener maps —
/// every live picker has an entry: `is_number_picker` drives the keypad
/// edit-mode filter, and the obj_ref is the `fireStep` dispatch target.
/// Entries are removed by the LV_EVENT_DELETE trampoline, so a recycled
/// `lv_obj_t*` from a later Activity can't be misidentified as a picker.
static mut PICKER_MAP: [(usize, u16); MAX_PICKERS] = [(0, 0); MAX_PICKERS];
static mut PICKER_MAP_LEN: usize = 0;

const STEP_QUEUE_SIZE: usize = 16;
static mut STEP_QUEUE: [(usize, i32); STEP_QUEUE_SIZE] = [(0, 0); STEP_QUEUE_SIZE];
static mut STEP_QUEUE_HEAD: usize = 0;
static mut STEP_QUEUE_TAIL: usize = 0;

unsafe extern "C" fn picker_defocused_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    if obj.is_null() {
        return;
    }
    // Focus moved away (programmatic requestFocus, Activity switch, …) while
    // possibly mid-edit: drop the edit outline and tell the edit-mode filter
    // to abandon, so A/B go back to navigating instead of stepping a widget
    // that no longer has focus.
    unsafe { lv_obj_remove_state(obj, LV_STATE_EDITED) };
    super::super::events::notify_picker_gone(obj as usize);
}

unsafe extern "C" fn picker_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let mut i = 0;
        while i < PICKER_MAP_LEN {
            if PICKER_MAP[i].0 == obj {
                PICKER_MAP[i] = PICKER_MAP[PICKER_MAP_LEN - 1];
                PICKER_MAP_LEN -= 1;
            } else {
                i += 1;
            }
        }
    }
    super::super::events::notify_picker_gone(obj);
}

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        // The theme's card style gives the same dark rounded box as EditText.
        // A value box never scrolls and never draws scrollbars.
        lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
        lv_obj_set_scrollbar_mode(o, LV_SCROLLBAR_MODE_OFF);
        // The label centers in the box (symmetric padding cancels out of the
        // centering math anyway); zero the card's large default padding like
        // LinearLayout does so it can't constrain small explicit sizes.
        lv_obj_set_style_pad_left(o, 0, 0);
        lv_obj_set_style_pad_right(o, 0, 0);
        lv_obj_set_style_pad_top(o, 0, 0);
        lv_obj_set_style_pad_bottom(o, 0, 0);
        // Plain lv_obj gets no theme focus styling; replicate the theme's
        // outline_primary (FOCUS_KEY) / outline_secondary (EDITED) — 3 px
        // outline + pad, 50% opacity, theme accent colors — so the picker's
        // focus and edit feedback matches Button/EditText. EDITED outranks
        // FOCUS_KEY in style specificity while both states are set.
        lv_obj_set_style_outline_width(o, 3, LV_STATE_FOCUS_KEY);
        lv_obj_set_style_outline_pad(o, 3, LV_STATE_FOCUS_KEY);
        lv_obj_set_style_outline_opa(o, 127, LV_STATE_FOCUS_KEY);
        lv_obj_set_style_outline_color(o, lv_theme_get_color_primary(o), LV_STATE_FOCUS_KEY);
        lv_obj_set_style_outline_width(o, 3, LV_STATE_EDITED);
        lv_obj_set_style_outline_pad(o, 3, LV_STATE_EDITED);
        lv_obj_set_style_outline_opa(o, 127, LV_STATE_EDITED);
        lv_obj_set_style_outline_color(o, lv_theme_get_color_secondary(o), LV_STATE_EDITED);

        let label = lv_label_create(o);
        lv_label_set_text(label, c"".as_ptr());
        lv_obj_center(label);

        // A plain lv_obj is not a group-default widget, so join the active
        // Activity's keypad focus group here — NumberPicker is focusable out
        // of the box, like Button and EditText. No-op on touch-only boards
        // (no group exists before the first Activity pushes one).
        let group = lv_group_get_default();
        if !group.is_null() {
            lv_group_add_obj(group, o);
        }

        lv_obj_add_event_cb(
            o,
            Some(picker_defocused_cb),
            LV_EVENT_DEFOCUSED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            o,
            Some(picker_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
        o
    };
    handle_table::register(ptr)
}

/// `NumberPicker.nativeSetText` backing: set the centered value label. The
/// label is always child 0 (created in [`create`] before anything else can
/// be parented into the box).
pub(in crate::system::picodroid::graphics) fn set_text(id: i32, text: &str) {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return;
    }
    let label = unsafe { lv_obj_get_child(container, 0) };
    if label.is_null() {
        return;
    }
    // i32 decimal (sign + 10 digits) fits comfortably; NUL-terminate.
    let mut buf = [0u8; 32];
    let len = text.len().min(31);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_label_set_text(label, buf.as_ptr() as *const c_char) };
}

/// `NumberPicker.nativeRegisterPicker` backing: record the Java object as the
/// `fireStep` dispatch target and mark the lv_obj as a picker for the keypad
/// edit-mode filter. Idempotent: re-registration refreshes the obj_ref.
pub(in crate::system::picodroid::graphics) fn register_picker(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        for entry in &mut PICKER_MAP[..PICKER_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if PICKER_MAP_LEN < MAX_PICKERS {
            PICKER_MAP[PICKER_MAP_LEN] = (raw_ptr, obj_ref);
            PICKER_MAP_LEN += 1;
        }
    }
}

/// Whether the widget at `raw_ptr` is a registered NumberPicker. Consulted by
/// the keypad edit-mode filter on every ENTER press.
#[cfg_attr(not(has_buttons), allow(dead_code))]
pub fn is_number_picker(raw_ptr: usize) -> bool {
    unsafe {
        for entry in &PICKER_MAP[..PICKER_MAP_LEN] {
            if entry.0 == raw_ptr {
                return true;
            }
        }
    }
    false
}

/// Queue one edit-mode step (+1/-1) for the picker at `raw_ptr`; drained by
/// `lifecycle::dispatch_number_picker_steps` into `NumberPicker.fireStep`.
#[cfg_attr(not(has_buttons), allow(dead_code))]
pub fn push_step(raw_ptr: usize, direction: i32) {
    unsafe {
        let next = (STEP_QUEUE_HEAD + 1) % STEP_QUEUE_SIZE;
        if next != STEP_QUEUE_TAIL {
            STEP_QUEUE[STEP_QUEUE_HEAD] = (raw_ptr, direction);
            STEP_QUEUE_HEAD = next;
        }
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_step_queue() -> Option<(usize, i32)> {
    unsafe {
        if STEP_QUEUE_TAIL == STEP_QUEUE_HEAD {
            return None;
        }
        let r = STEP_QUEUE[STEP_QUEUE_TAIL];
        STEP_QUEUE_TAIL = (STEP_QUEUE_TAIL + 1) % STEP_QUEUE_SIZE;
        Some(r)
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_picker_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &PICKER_MAP[..PICKER_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_number_picker_state() {
    unsafe {
        PICKER_MAP_LEN = 0;
        STEP_QUEUE_HEAD = 0;
        STEP_QUEUE_TAIL = 0;
    }
}

/// Visit the Java `NumberPicker` object ref of every registered picker so the
/// GC keeps it alive. A picker referenced only by this native map (a local in
/// an Activity's `onCreate`, kept alive only natively by `addView`) would
/// otherwise be swept on the first GC, its slot reused, and a later
/// `fireStep` dispatch resolves a dead ref → `NoSuchMethod`. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_picker_roots(visit: &mut dyn FnMut(u16)) {
    unsafe {
        for &(_, r) in &PICKER_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
    }
}
