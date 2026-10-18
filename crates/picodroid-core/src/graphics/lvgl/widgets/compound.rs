// SPDX-License-Identifier: GPL-3.0-only
//! Plumbing several widgets share: the checked state every two-state widget
//! keeps the same way — `Switch`, `CheckBox`, `RadioButton` and
//! `ToggleButton` all hold it as LVGL's `LV_STATE_CHECKED` on the base
//! object.

use crate::lvgl_ffi::*;

use super::super::handle_table;

/// Whether `id`'s widget is checked; `false` for a stale handle.
pub(super) fn is_checked(id: i32) -> bool {
    handle_table::get(id).is_some_and(|o| lv_obj_has_state(o, LV_STATE_CHECKED))
}

/// Set or clear `id`'s checked state. The widget, unless the handle is
/// stale.
pub(super) fn set_checked(id: i32, checked: bool) -> Option<Obj> {
    let o = handle_table::get(id)?;
    if checked {
        lv_obj_add_state(o, LV_STATE_CHECKED);
    } else {
        lv_obj_remove_state(o, LV_STATE_CHECKED);
    }
    Some(o)
}

/// Flip `id`'s checked state. The widget, unless the handle is stale.
pub(super) fn toggle(id: i32) -> Option<Obj> {
    let o = handle_table::get(id)?;
    if lv_obj_has_state(o, LV_STATE_CHECKED) {
        lv_obj_remove_state(o, LV_STATE_CHECKED);
    } else {
        lv_obj_add_state(o, LV_STATE_CHECKED);
    }
    Some(o)
}

/// `performClick` on a two-state widget: flip it and send the
/// `VALUE_CHANGED` a tap would have.
pub(super) fn perform_checked_change(id: i32) {
    if let Some(o) = toggle(id) {
        send_event(o, Notice::ValueChanged);
    }
}
