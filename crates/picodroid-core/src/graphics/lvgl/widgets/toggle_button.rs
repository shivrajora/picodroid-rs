// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ToggleButton` (LVGL `lv_button` w/ `LV_OBJ_FLAG_CHECKABLE`
//! and a child label whose text swaps on state change).
//!
//! Maintains a side-table of `(raw_ptr, text_on, text_off)` entries
//! because LVGL doesn't store the off-state label text natively.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{map_mut, map_ref, warn_full, PtrMap, Upsert};

const MAX_TOGGLE_BUTTONS: usize = 16;
const TEXT_BUF_SIZE: usize = 32;

struct Entry {
    /// Raw `lv_obj_t*` cast to `usize` — matches LVGL callback values.
    raw_ptr: usize,
    text_on: [u8; TEXT_BUF_SIZE],
    text_off: [u8; TEXT_BUF_SIZE],
}

impl Entry {
    const fn empty() -> Self {
        Self {
            raw_ptr: 0,
            text_on: [0; TEXT_BUF_SIZE],
            text_off: [0; TEXT_BUF_SIZE],
        }
    }
}

const EMPTY_ENTRY: Entry = Entry::empty();

static mut ENTRIES: [Entry; MAX_TOGGLE_BUTTONS] = [EMPTY_ENTRY; MAX_TOGGLE_BUTTONS];
static mut ENTRY_COUNT: usize = 0;

const QUEUE_SIZE: usize = 16;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

const MAX_LISTENERS: usize = 32;
static mut HANDLE_MAP: PtrMap<MAX_LISTENERS> = PtrMap::new();

/// Remove the dying widget from both per-widget tables: the listener map and
/// the `ENTRIES` label side-table (a stale `raw_ptr` there would relabel a
/// recycled address).
unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        map_mut(&raw mut HANDLE_MAP).remove(obj);
        let entries = &raw mut ENTRIES;
        let entries = &mut *entries;
        let mut i = 0;
        while i < ENTRY_COUNT {
            if entries[i].raw_ptr == obj {
                entries.swap(i, ENTRY_COUNT - 1);
                entries[ENTRY_COUNT - 1] = Entry::empty();
                ENTRY_COUNT -= 1;
            } else {
                i += 1;
            }
        }
    }
}

unsafe fn find_entry(raw_ptr: usize) -> Option<&'static mut Entry> {
    unsafe {
        ENTRIES[..ENTRY_COUNT]
            .iter_mut()
            .find(|e| e.raw_ptr == raw_ptr)
    }
}

unsafe fn update_label(obj: *mut lv_obj_t) {
    unsafe {
        if let Some(entry) = find_entry(obj as usize) {
            let label = lv_obj_get_child(obj, 0);
            if !label.is_null() {
                let checked = lv_obj_has_state(obj, LV_STATE_CHECKED);
                let text = if checked {
                    &entry.text_on
                } else {
                    &entry.text_off
                };
                lv_label_set_text(label, text.as_ptr() as *const c_char);
            }
        }
    }
}

unsafe fn register_entry(raw_ptr: usize, text_on: &[u8], text_off: &[u8]) {
    unsafe {
        if ENTRY_COUNT >= MAX_TOGGLE_BUTTONS {
            warn_full("toggle-button-entries");
            return;
        }
        let entry = &mut ENTRIES[ENTRY_COUNT];
        entry.raw_ptr = raw_ptr;
        let on_len = text_on.len().min(TEXT_BUF_SIZE - 1);
        entry.text_on[..on_len].copy_from_slice(&text_on[..on_len]);
        entry.text_on[on_len] = 0;
        let off_len = text_off.len().min(TEXT_BUF_SIZE - 1);
        entry.text_off[..off_len].copy_from_slice(&text_off[..off_len]);
        entry.text_off[off_len] = 0;
        ENTRY_COUNT += 1;
    }
}

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    unsafe {
        update_label(obj);
        let next = (QUEUE_HEAD + 1) % QUEUE_SIZE;
        if next != QUEUE_TAIL {
            QUEUE[QUEUE_HEAD] = obj as usize;
            QUEUE_HEAD = next;
        }
    }
}

fn create_internal(text_on: &[u8], text_off: &[u8]) -> i32 {
    let btn = unsafe { lv_button_create(lifecycle::screen_ptr()) };
    unsafe {
        lv_obj_add_flag(btn, LV_OBJ_FLAG_CHECKABLE);
        let label = lv_label_create(btn);
        // Initial label shows the off-state text.
        let mut buf = [0u8; TEXT_BUF_SIZE];
        let len = text_off.len().min(TEXT_BUF_SIZE - 1);
        buf[..len].copy_from_slice(&text_off[..len]);
        buf[len] = 0;
        lv_label_set_text(label, buf.as_ptr() as *const c_char);
        lv_obj_center(label);
        lv_obj_add_event_cb(
            btn,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            btn,
            Some(map_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
        register_entry(btn as usize, text_on, text_off);
    }
    handle_table::register(btn)
}

pub(in crate::graphics) fn create() -> i32 {
    create_internal(b"ON", b"OFF")
}

pub(in crate::graphics) fn create_with_text(text_on: &str, text_off: &str) -> i32 {
    create_internal(text_on.as_bytes(), text_off.as_bytes())
}

pub(in crate::graphics) fn is_checked(id: i32) -> bool {
    unsafe { lv_obj_has_state(handle_table::lookup(id), LV_STATE_CHECKED) }
}

pub(in crate::graphics) fn set_checked(id: i32, checked: bool) {
    unsafe {
        let obj = handle_table::lookup(id);
        if checked {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        }
        update_label(obj);
    }
}

pub(in crate::graphics) fn toggle(id: i32) {
    unsafe {
        let obj = handle_table::lookup(id);
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
        update_label(obj);
    }
}

pub(in crate::graphics) fn perform_checked_change(id: i32) {
    unsafe {
        let obj = handle_table::lookup(id);
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
        lv_obj_send_event(obj, LV_EVENT_VALUE_CHANGED, core::ptr::null_mut());
    }
}

fn copy_into(dst: &mut [u8; TEXT_BUF_SIZE], src: &str) {
    let len = src.len().min(TEXT_BUF_SIZE - 1);
    dst[..len].copy_from_slice(&src.as_bytes()[..len]);
    dst[len] = 0;
}

pub(in crate::graphics) fn set_text_on(id: i32, text: &str) {
    unsafe {
        let obj = handle_table::lookup(id);
        if let Some(entry) = find_entry(obj as usize) {
            copy_into(&mut entry.text_on, text);
        }
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            update_label(obj);
        }
    }
}

pub(in crate::graphics) fn set_text_off(id: i32, text: &str) {
    unsafe {
        let obj = handle_table::lookup(id);
        if let Some(entry) = find_entry(obj as usize) {
            copy_into(&mut entry.text_off, text);
        }
        if !lv_obj_has_state(obj, LV_STATE_CHECKED) {
            update_label(obj);
        }
    }
}

pub(in crate::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        if let Upsert::Full = map_mut(&raw mut HANDLE_MAP).upsert(raw_ptr, obj_ref) {
            warn_full("toggle-checked");
        }
    }
}

pub fn drain_checked_change_queue() -> Option<usize> {
    unsafe {
        if QUEUE_TAIL == QUEUE_HEAD {
            return None;
        }
        let h = QUEUE[QUEUE_TAIL];
        QUEUE_TAIL = (QUEUE_TAIL + 1) % QUEUE_SIZE;
        Some(h)
    }
}

pub fn lookup_checked_change_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const HANDLE_MAP).lookup(handle) }
}

pub fn reset_toggle_button_state() {
    unsafe {
        ENTRY_COUNT = 0;
        let entries = &raw mut ENTRIES;
        *entries = [EMPTY_ENTRY; MAX_TOGGLE_BUTTONS];
        map_mut(&raw mut HANDLE_MAP).reset();
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}

/// Visit the Java `ToggleButton` object ref of every toggle button registered
/// for a checked-change listener so the GC keeps it alive. A `ToggleButton`
/// referenced only by this native map (no Java field; `addView` keeps it alive
/// only natively) would otherwise be swept on the first GC, its slot reused, and
/// a later dispatch resolves a dead ref → `NoSuchMethod`. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_checked_change_listener_roots(visit: &mut dyn FnMut(u16)) {
    unsafe { map_ref(&raw const HANDLE_MAP).visit(visit) }
}
