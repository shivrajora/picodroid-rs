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
use super::super::style_batch;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

struct State {
    /// `(picker handle, +1/-1)`: the steps the keypad edit mode asked for.
    /// The `NumberPicker` constructor binds every instance to its box
    /// (`nativeRegisterPicker`), so the drain finds its `fireStep` target
    /// through the handle table; the box itself carries
    /// `LV_OBJ_FLAG_USER_2` for the edit-mode filter to recognise it.
    step_queue: LocalRing<(i32, i32), STEP_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        step_queue: LocalRing::new((0, 0)),
    })
};

const STEP_QUEUE_SIZE: usize = 16;

lv_event_cb! {
    fn picker_defocused_cb(e) {
        let obj = e.target();
        // Focus moved away (programmatic requestFocus, Activity switch, …)
        // while possibly mid-edit: drop the edit outline and tell the
        // edit-mode filter to abandon, so A/B go back to navigating instead
        // of stepping a widget that no longer has focus.
        lv_obj_remove_state(obj, LV_STATE_EDITED);
        super::super::events::notify_picker_gone(obj.addr());
    }
}

lv_event_cb! {
    fn picker_delete_cb(e) {
        super::super::events::notify_picker_gone(e.target().addr());
    }
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(o) = lifecycle::screen_ptr().and_then(|o| lv_obj_create(o)) else {
        return 0;
    };
    {
        // The theme's card style gives the same dark rounded box as EditText.
        // A value box never scrolls and never draws scrollbars.
        lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
        lv_obj_set_scrollbar_mode(o, LV_SCROLLBAR_MODE_OFF);
        // What tells the keypad's edit-mode filter a picker from any other
        // plain box (`is_number_picker`).
        lv_obj_add_flag(o, LV_OBJ_FLAG_USER_2);
        // The style sets below under one refresh (style_batch.rs); the label
        // is created after them because a create switches the refresh back on.
        style_batch::with_one_refresh(o, LV_STYLE_PAD_TOP, || {
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
            // The group's quiet focus — the first stop of a screen that opens
            // on a picker — sets FOCUSED alone; FOCUS_KEY joins once a key
            // moves the focus. The same outline for that state, where a key
            // can move it (groups.rs::ensure_in_group), so the focus is seen
            // before the first key (QA round 2, R4).
            if crate::board_cfg::input::HAS_NAV_KEYS {
                lv_obj_set_style_outline_width(o, 3, LV_STATE_FOCUSED);
                lv_obj_set_style_outline_pad(o, 3, LV_STATE_FOCUSED);
                lv_obj_set_style_outline_opa(o, 127, LV_STATE_FOCUSED);
                lv_obj_set_style_outline_color(o, lv_theme_get_color_primary(o), LV_STATE_FOCUSED);
            }
        });

        if let Some(label) = lv_label_create(o) {
            // SAFETY: `label` was just made by `lv_label_create`; the text
            // is a NUL-terminated literal LVGL copies.
            unsafe { lv_label_set_text(label, c"".as_ptr()) };
            lv_obj_center(label);
        }

        // A plain lv_obj is not a group-default widget, so join the active
        // Activity's keypad focus group here — NumberPicker is focusable out
        // of the box, like Button and EditText. No-op on touch-only boards
        // (no group exists before the first Activity pushes one).
        if let Some(group) = lv_group_get_default() {
            lv_group_add_obj(group, o);
        }

        add_event_cb(o, picker_defocused_cb, LV_EVENT_DEFOCUSED);
        add_event_cb(o, picker_delete_cb, LV_EVENT_DELETE);
    }
    handle_table::register(o)
}

/// `NumberPicker.nativeSetText` backing: set the centered value label. The
/// label is always child 0 (created in [`create`] before anything else can
/// be parented into the box).
pub(in crate::graphics) fn set_text(id: i32, text: &str) {
    let Some(container) = handle_table::get(id) else {
        return;
    };
    let Some(label) = lv_obj_get_child(container, 0) else {
        return;
    };
    // i32 decimal (sign + 10 digits) fits comfortably; NUL-terminate.
    let mut buf = [0u8; 32];
    let len = text.len().min(31);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    // SAFETY: a `NumberPicker` native: its constructors give `View` the box
    // [`create`] made, whose only child is the `lv_label` made there (a
    // NumberPicker is no ViewGroup); `buf` is NUL-terminated and LVGL
    // copies it.
    unsafe { lv_label_set_text(label, buf.as_ptr() as *const c_char) };
}

/// Whether `obj` is a NumberPicker's box. Consulted by the keypad edit-mode
/// filter on every ENTER press.
pub fn is_number_picker(obj: Obj) -> bool {
    lv_obj_has_flag(obj, LV_OBJ_FLAG_USER_2)
}

/// Queue one edit-mode step (+1/-1) for the picker behind `handle`; drained
/// by `lifecycle::dispatch_number_picker_steps` into `NumberPicker.fireStep`.
pub fn push_step(handle: i32, direction: i32) {
    if handle != 0 {
        STATE.step_queue.push((handle, direction));
    }
}

pub fn drain_step_queue() -> Option<(i32, i32)> {
    STATE.step_queue.pop()
}

pub fn reset_number_picker_state() {
    STATE.step_queue.clear();
}
