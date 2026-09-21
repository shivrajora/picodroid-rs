// SPDX-License-Identifier: GPL-3.0-only
//! The view focus-change listener registry and its event queue.

use super::*;

// ── View focus-change listener registry ─────────────────────────────────────
//
// Backs `android.view.View.OnFocusChangeListener`. Mirrors the swipe pattern:
// a `(handle, obj_ref)` map keyed by raw `lv_obj_t*` plus a ring buffer of
// `(handle, has_focus)` records fed by `LV_EVENT_FOCUSED`/`LV_EVENT_DEFOCUSED`
// trampolines. The framework loop drains the queue and invokes
// `View.fireFocusChange(boolean)` on the matching object. A view only emits
// these once it is a member of the active Activity's keypad focus group
// (setFocusable/requestFocus or an adapter row), which is exactly when Android
// would deliver focus callbacks.

pub(super) const MAX_FOCUS_LISTENERS: usize = 32;

#[derive(Copy, Clone)]
pub struct FocusRecord {
    pub view_handle: usize,
    pub has_focus: bool,
}

pub(super) const FOCUS_QUEUE_SIZE: usize = 16;
pub(super) static mut FOCUS_QUEUE: [FocusRecord; FOCUS_QUEUE_SIZE] = [FocusRecord {
    view_handle: 0,
    has_focus: false,
}; FOCUS_QUEUE_SIZE];
pub(super) static mut FOCUS_QUEUE_HEAD: usize = 0;
pub(super) static mut FOCUS_QUEUE_TAIL: usize = 0;

pub(super) static mut VIEW_FOCUS_MAP: PtrMap<MAX_FOCUS_LISTENERS> = PtrMap::new();

pub(super) unsafe extern "C" fn focus_map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe { map_mut(&raw mut VIEW_FOCUS_MAP).remove(obj) }
}

pub(super) fn push_focus_event(handle: usize, has_focus: bool) {
    unsafe {
        let head = FOCUS_QUEUE_HEAD;
        let next = (head + 1) % FOCUS_QUEUE_SIZE;
        if next != FOCUS_QUEUE_TAIL {
            FOCUS_QUEUE[head] = FocusRecord {
                view_handle: handle,
                has_focus,
            };
            FOCUS_QUEUE_HEAD = next;
        }
    }
}

pub(super) unsafe extern "C" fn view_focused_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    push_focus_event(obj, true);
}

pub(super) unsafe extern "C" fn view_defocused_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    push_focus_event(obj, false);
}

/// `View.nativeIsFocused()` backing — whether this view is the active keypad
/// group's focused widget. Mirrors `android.view.View#isFocused`. Cheap:
/// reuses the `lv_group_get_focused(group) == raw` check from
/// [`request_view_focus`].
pub fn view_is_focused(id: i32) -> bool {
    let raw = super::super::handle_table::lookup(id);
    if raw.is_null() {
        return false;
    }
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return false;
        }
        lv_group_get_focused(group) == raw
    }
}

/// `View.setOnFocusChangeListener` backing: record the Java `View` as the
/// focus-change target and attach the LVGL FOCUSED/DEFOCUSED trampolines.
/// Idempotent — re-registration just refreshes the obj_ref slot.
pub fn register_view_focus_change_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        match map_mut(&raw mut VIEW_FOCUS_MAP).upsert(raw_ptr, obj_ref) {
            Upsert::Updated => return,
            Upsert::Full => {
                warn_full("view-focus");
                return;
            }
            Upsert::Inserted => {}
        }
        lv_obj_add_event_cb(
            raw_obj,
            Some(view_focused_cb),
            LV_EVENT_FOCUSED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(view_defocused_cb),
            LV_EVENT_DEFOCUSED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(focus_map_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
    }
}

pub fn drain_focus_change_event() -> Option<FocusRecord> {
    unsafe {
        if FOCUS_QUEUE_TAIL == FOCUS_QUEUE_HEAD {
            return None;
        }
        let r = FOCUS_QUEUE[FOCUS_QUEUE_TAIL];
        FOCUS_QUEUE_TAIL = (FOCUS_QUEUE_TAIL + 1) % FOCUS_QUEUE_SIZE;
        Some(r)
    }
}

pub fn lookup_focus_view_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const VIEW_FOCUS_MAP).lookup(handle) }
}

pub fn reset_view_focus_listener_state() {
    unsafe {
        map_mut(&raw mut VIEW_FOCUS_MAP).reset();
        FOCUS_QUEUE_HEAD = 0;
        FOCUS_QUEUE_TAIL = 0;
    }
}
