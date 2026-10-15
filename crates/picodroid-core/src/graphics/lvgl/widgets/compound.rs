// SPDX-License-Identifier: GPL-3.0-only
//! Plumbing several widgets share: the checked state every two-state widget
//! keeps the same way — `Switch`, `CheckBox`, `RadioButton` and
//! `ToggleButton` all hold it as LVGL's `LV_STATE_CHECKED` on the base
//! object — and the listener registration.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::listener_map::{warn_full, PtrMap, Upsert};

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

/// Record `obj_ref` as the checked-change listener of `id`'s widget in
/// `map`; a no-op for a stale handle. `what` names the map in the warning a
/// full map logs.
pub(super) fn register_listener<const N: usize>(
    map: &PtrMap<N>,
    id: i32,
    obj_ref: u16,
    what: &'static str,
) {
    let Some(o) = handle_table::get(id) else {
        return;
    };
    if let Upsert::Full = map.upsert(o.addr(), obj_ref) {
        warn_full(what);
    }
}

/// [`register_listener`] for a map whose entries a delete hook removes: on
/// the first registration of a widget, `delete_cb` goes on its
/// `LV_EVENT_DELETE` so a recycled address can't alias a dead widget's
/// listener entry.
pub(super) fn register_listener_with_delete_hook<const N: usize>(
    map: &PtrMap<N>,
    id: i32,
    obj_ref: u16,
    delete_cb: unsafe extern "C" fn(*mut lv_event_t),
    what: &'static str,
) {
    let Some(o) = handle_table::get(id) else {
        return;
    };
    match map.upsert(o.addr(), obj_ref) {
        Upsert::Updated => {}
        Upsert::Full => warn_full(what),
        Upsert::Inserted => add_event_cb(o, delete_cb, LV_EVENT_DELETE),
    }
}
