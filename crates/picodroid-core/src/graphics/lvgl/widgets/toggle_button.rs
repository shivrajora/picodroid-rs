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
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

const MAX_TOGGLE_BUTTONS: usize = 16;
const TEXT_BUF_SIZE: usize = 32;

#[derive(Copy, Clone)]
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

const QUEUE_SIZE: usize = 16;

struct State {
    /// The label texts, `entries[..entry_count]`.
    entries: [Cell<Entry>; MAX_TOGGLE_BUTTONS],
    entry_count: Cell<usize>,
    /// Checked-change events, each the widget's handle.
    queue: LocalRing<i32, QUEUE_SIZE>,
}

// SAFETY: filled by natives and LVGL callbacks, drained by natives, all on
// core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        entries: [const { Cell::new(EMPTY_ENTRY) }; MAX_TOGGLE_BUTTONS],
        entry_count: Cell::new(0),
        queue: LocalRing::new(0),
    })
};

lv_event_cb! {
    /// Remove the dying widget from the `entries` label side-table (a stale
    /// `raw_ptr` there would relabel a recycled address).
    fn entries_delete_cb(e) {
        let obj = e.target().addr();
        let entries = &STATE.entries;
        let mut i = 0;
        while i < STATE.entry_count.get() {
            let last = STATE.entry_count.get() - 1;
            if entries[i].get().raw_ptr == obj {
                entries[i].set(entries[last].get());
                entries[last].set(Entry::empty());
                STATE.entry_count.set(last);
            } else {
                i += 1;
            }
        }
    }
}

/// The side-table cell of the toggle button at `raw_ptr`.
fn find_entry(raw_ptr: usize) -> Option<&'static Cell<Entry>> {
    STATE.entries[..STATE.entry_count.get()]
        .iter()
        .find(|e| e.get().raw_ptr == raw_ptr)
}

/// Show the text for `obj`'s checked state on its label.
fn update_label(obj: Obj) {
    let Some(entry) = find_entry(obj.addr()).map(Cell::get) else {
        return;
    };
    let Some(label) = lv_obj_get_child(obj, 0) else {
        return;
    };
    let text = if lv_obj_has_state(obj, LV_STATE_CHECKED) {
        &entry.text_on
    } else {
        &entry.text_off
    };
    // SAFETY: `obj` has an entry, so it is a toggle button `create_internal`
    // made, whose only child is the `lv_label` made there (a ToggleButton is
    // no ViewGroup); both texts are NUL-terminated by `register_entry` and
    // `copy_into`, and LVGL copies them.
    unsafe { lv_label_set_text(label, text.as_ptr() as *const c_char) };
}

fn register_entry(raw_ptr: usize, text_on: &[u8], text_off: &[u8]) {
    let count = STATE.entry_count.get();
    if count >= MAX_TOGGLE_BUTTONS {
        crate::pd_warn!("ToggleButton: the label table is full, a button keeps its first text");
        return;
    }
    let cell = &STATE.entries[count];
    let mut entry = cell.get();
    entry.raw_ptr = raw_ptr;
    let on_len = text_on.len().min(TEXT_BUF_SIZE - 1);
    entry.text_on[..on_len].copy_from_slice(&text_on[..on_len]);
    entry.text_on[on_len] = 0;
    let off_len = text_off.len().min(TEXT_BUF_SIZE - 1);
    entry.text_off[..off_len].copy_from_slice(&text_off[..off_len]);
    entry.text_off[off_len] = 0;
    cell.set(entry);
    STATE.entry_count.set(count + 1);
}

lv_event_cb! {
    fn value_changed_cb(e) {
        let obj = e.target();
        update_label(obj);
        let handle = handle_table::handle_of(obj);
        if handle != 0 {
            STATE.queue.push(handle);
        }
    }
}

fn create_internal(text_on: &[u8], text_off: &[u8]) -> i32 {
    let Some(btn) = lifecycle::screen_ptr().and_then(|o| lv_button_create(o)) else {
        return 0;
    };
    lv_obj_add_flag(btn, LV_OBJ_FLAG_CHECKABLE);
    if let Some(label) = lv_label_create(btn) {
        // Initial label shows the off-state text.
        let mut buf = [0u8; TEXT_BUF_SIZE];
        let len = text_off.len().min(TEXT_BUF_SIZE - 1);
        buf[..len].copy_from_slice(&text_off[..len]);
        buf[len] = 0;
        // SAFETY: `label` was just made by `lv_label_create`; `buf` is
        // NUL-terminated and LVGL copies it.
        unsafe { lv_label_set_text(label, buf.as_ptr() as *const c_char) };
        lv_obj_center(label);
    }
    add_event_cb(btn, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    add_event_cb(btn, entries_delete_cb, LV_EVENT_DELETE);
    register_entry(btn.addr(), text_on, text_off);
    handle_table::register(btn)
}

pub(in crate::graphics) fn create() -> i32 {
    create_internal(b"ON", b"OFF")
}

pub(in crate::graphics) fn create_with_text(text_on: &str, text_off: &str) -> i32 {
    create_internal(text_on.as_bytes(), text_off.as_bytes())
}

pub(in crate::graphics) fn is_checked(id: i32) -> bool {
    super::compound::is_checked(id)
}

pub(in crate::graphics) fn set_checked(id: i32, checked: bool) {
    if let Some(obj) = super::compound::set_checked(id, checked) {
        update_label(obj);
    }
}

pub(in crate::graphics) fn toggle(id: i32) {
    if let Some(obj) = super::compound::toggle(id) {
        update_label(obj);
    }
}

pub(in crate::graphics) fn perform_checked_change(id: i32) {
    super::compound::perform_checked_change(id);
}

fn copy_into(dst: &mut [u8; TEXT_BUF_SIZE], src: &str) {
    let len = src.len().min(TEXT_BUF_SIZE - 1);
    dst[..len].copy_from_slice(&src.as_bytes()[..len]);
    dst[len] = 0;
}

pub(in crate::graphics) fn set_text_on(id: i32, text: &str) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    if let Some(cell) = find_entry(obj.addr()) {
        let mut entry = cell.get();
        copy_into(&mut entry.text_on, text);
        cell.set(entry);
    }
    if lv_obj_has_state(obj, LV_STATE_CHECKED) {
        update_label(obj);
    }
}

pub(in crate::graphics) fn set_text_off(id: i32, text: &str) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    if let Some(cell) = find_entry(obj.addr()) {
        let mut entry = cell.get();
        copy_into(&mut entry.text_off, text);
        cell.set(entry);
    }
    if !lv_obj_has_state(obj, LV_STATE_CHECKED) {
        update_label(obj);
    }
}

pub fn drain_checked_change_queue() -> Option<i32> {
    STATE.queue.pop()
}

pub fn reset_toggle_button_state() {
    STATE.entry_count.set(0);
    for entry in &STATE.entries {
        entry.set(EMPTY_ENTRY);
    }
    STATE.queue.clear();
}
