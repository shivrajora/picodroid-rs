// SPDX-License-Identifier: GPL-3.0-only
//! The view focus-change event queue.

use super::*;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

// ── View focus-change events ────────────────────────────────────────────────
//
// Backs `android.view.View.OnFocusChangeListener`. A ring buffer of
// `(handle, has_focus)` records fed by `LV_EVENT_FOCUSED`/`LV_EVENT_DEFOCUSED`
// trampolines. The framework loop drains the queue and invokes
// `View.fireFocusChange(boolean)` on the view the handle table binds to the
// handle. A view only emits these once it is a member of the active
// Activity's keypad focus group (setFocusable/requestFocus or an adapter
// row), which is exactly when Android would deliver focus callbacks.

#[derive(Copy, Clone)]
pub struct FocusRecord {
    /// The widget's `nativeHandle`.
    pub view: i32,
    pub has_focus: bool,
}

pub(super) const FOCUS_QUEUE_SIZE: usize = 16;

pub(super) struct Focus {
    queue: LocalRing<FocusRecord, FOCUS_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL's focus callbacks and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
pub(super) static FOCUS: Core0<Focus> = unsafe {
    Core0::new(Focus {
        queue: LocalRing::new(FocusRecord {
            view: 0,
            has_focus: false,
        }),
    })
};

pub(super) fn push_focus_event(obj: Obj, has_focus: bool) {
    let view = super::super::handle_table::handle_of(obj);
    if view == 0 {
        return;
    }
    FOCUS.queue.push(FocusRecord { view, has_focus });
}

lv_event_cb! {
    pub(super) fn view_focused_cb(e) {
        push_focus_event(e.target(), true);
    }
}

lv_event_cb! {
    pub(super) fn view_defocused_cb(e) {
        push_focus_event(e.target(), false);
    }
}

/// `View.nativeIsFocused()` backing — whether this view is the active keypad
/// group's focused widget. Mirrors `android.view.View#isFocused`. Cheap:
/// reuses the `lv_group_get_focused(group) == raw` check from
/// [`request_view_focus`].
pub fn view_is_focused(id: i32) -> bool {
    let Some(raw) = super::super::handle_table::get(id) else {
        return false;
    };
    lv_group_get_default().and_then(|o| lv_group_get_focused(o)) == Some(raw)
}

/// `View.setOnFocusChangeListener`'s widget half: attach the LVGL
/// FOCUSED/DEFOCUSED trampolines to `id`'s widget. Idempotent, and a no-op
/// for a released view.
pub fn attach_focus_change(id: i32) {
    let Some(raw_obj) = super::super::handle_table::get(id) else {
        return;
    };
    lv_obj_remove_event_cb(raw_obj, Some(view_focused_cb));
    add_event_cb(raw_obj, view_focused_cb, LV_EVENT_FOCUSED);
    lv_obj_remove_event_cb(raw_obj, Some(view_defocused_cb));
    add_event_cb(raw_obj, view_defocused_cb, LV_EVENT_DEFOCUSED);
}

pub fn drain_focus_change_event() -> Option<FocusRecord> {
    FOCUS.queue.pop()
}

pub fn reset_view_focus_listener_state() {
    FOCUS.queue.clear();
}
