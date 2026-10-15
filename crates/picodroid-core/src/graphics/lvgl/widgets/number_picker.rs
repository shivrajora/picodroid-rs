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
use super::super::listener_map::PtrMap;
use super::super::style_batch;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const MAX_PICKERS: usize = 16;
struct State {
    /// (container raw ptr, Java obj_ref). Registered from the `NumberPicker`
    /// constructor via `nativeRegisterPicker`, so — unlike the listener maps —
    /// every live picker has an entry: `is_number_picker` drives the keypad
    /// edit-mode filter, and the obj_ref is the `fireStep` dispatch target.
    /// Entries are removed by the LV_EVENT_DELETE trampoline, so a recycled
    /// `lv_obj_t*` from a later Activity can't be misidentified as a picker.
    picker_map: PtrMap<MAX_PICKERS>,
    step_queue: LocalRing<(usize, i32), STEP_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        picker_map: PtrMap::new(),
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
        let obj = e.target().addr();
        STATE.picker_map.remove(obj);
        super::super::events::notify_picker_gone(obj);
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

/// `NumberPicker.nativeRegisterPicker` backing: record the Java object as the
/// `fireStep` dispatch target and mark the lv_obj as a picker for the keypad
/// edit-mode filter. Idempotent: re-registration refreshes the obj_ref.
pub(in crate::graphics) fn register_picker(id: i32, obj_ref: u16) {
    super::compound::register_listener(&STATE.picker_map, id, obj_ref, "number-picker");
}

/// Whether the widget at `raw_ptr` is a registered NumberPicker. Consulted by
/// the keypad edit-mode filter on every ENTER press.
pub fn is_number_picker(raw_ptr: usize) -> bool {
    STATE.picker_map.lookup(raw_ptr).is_some()
}

/// Queue one edit-mode step (+1/-1) for the picker at `raw_ptr`; drained by
/// `lifecycle::dispatch_number_picker_steps` into `NumberPicker.fireStep`.
pub fn push_step(raw_ptr: usize, direction: i32) {
    STATE.step_queue.push((raw_ptr, direction));
}

pub fn drain_step_queue() -> Option<(usize, i32)> {
    STATE.step_queue.pop()
}

pub fn lookup_picker_obj(handle: usize) -> Option<u16> {
    STATE.picker_map.lookup(handle)
}

pub fn reset_number_picker_state() {
    STATE.picker_map.reset();
    STATE.step_queue.clear();
}

/// Visit the Java `NumberPicker` object ref of every registered picker so the
/// GC keeps it alive. A picker referenced only by this native map (a local in
/// an Activity's `onCreate`, kept alive only natively by `addView`) would
/// otherwise be swept on the first GC, its slot reused, and a later
/// `fireStep` dispatch resolves a dead ref → `NoSuchMethod`. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_picker_roots(visit: &mut dyn FnMut(u16)) {
    STATE.picker_map.visit(visit)
}
