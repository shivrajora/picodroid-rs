// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ListView` (LVGL `lv_list`).
//!
//! Rows are created with `lv_list_add_button` (a focusable, group-def,
//! clickable `lv_button`) rather than the non-focusable `lv_list_add_text`,
//! so on a hardware-button board the per-Activity keypad group can traverse
//! the rows (PREV/NEXT move the focus highlight) and ENTER activates the
//! focused row. Each row carries an `LV_EVENT_CLICKED` trampoline that
//! enqueues the row pointer; the main loop drains it and fires
//! `ListView.fireItemClick(position)` on the Java `ListView` registered for
//! the row's parent list.
//!
//! This mirrors the click pathway in `widgets/button.rs` and the listener
//! map in `widgets/spinner.rs`. Only the *list* is mapped to a Java object
//! (one entry per `ListView`); the row's position is recovered at click time
//! by scanning the list's children, so there is no per-row table to size or
//! invalidate (`lv_obj_get_index` is not bound, hence the linear scan).

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

/// Accent fill (RGB888) for the keypad-focused list row — a material-teal that
/// reads clearly as "selected" on both light and dark surfaces. The default
/// theme's focus styling is too subtle to rely on for no-touch navigation.
const FOCUS_HIGHLIGHT_RGB: u32 = 0x0026_A69A;

// ── Item-click event queue (raw row `lv_obj_t*` pointers) ───────────────────

const ITEM_CLICK_QUEUE_SIZE: usize = 16;
static mut ITEM_CLICK_QUEUE: [usize; ITEM_CLICK_QUEUE_SIZE] = [0; ITEM_CLICK_QUEUE_SIZE];
static mut ITEM_CLICK_QUEUE_HEAD: usize = 0;
static mut ITEM_CLICK_QUEUE_TAIL: usize = 0;

// ── ListView handle → Java object mapping (one entry per ListView) ──────────

const MAX_LIST_LISTENERS: usize = 16;
static mut LISTENER_MAP: [(usize, u16); MAX_LIST_LISTENERS] = [(0, 0); MAX_LIST_LISTENERS];
static mut LISTENER_MAP_LEN: usize = 0;

unsafe extern "C" fn row_click_cb(e: *mut lv_event_t) {
    let row = unsafe { lv_event_get_target_obj(e) };
    unsafe {
        let next = (ITEM_CLICK_QUEUE_HEAD + 1) % ITEM_CLICK_QUEUE_SIZE;
        if next != ITEM_CLICK_QUEUE_TAIL {
            ITEM_CLICK_QUEUE[ITEM_CLICK_QUEUE_HEAD] = row as usize;
            ITEM_CLICK_QUEUE_HEAD = next;
        }
    }
}

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_list_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn add_item(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        let list = handle_table::lookup(id);
        let row = lv_list_add_button(list, core::ptr::null(), buf.as_ptr() as *const c_char);
        // Make the row keypad-traversable: join the active Activity focus
        // group. Idempotent if `lv_list_add_button` already auto-joined it, and
        // a no-op when no group is active (non-button boards, or before the
        // first Activity launches), matching `events::set_view_focusable`.
        let group = lv_group_get_default();
        if !group.is_null() {
            lv_group_add_obj(group, row);
        }
        // Every row is clickable; the trampoline enqueues the row pointer. The
        // drain side no-ops if the row's list has no registered item listener.
        lv_obj_add_event_cb(
            row,
            Some(row_click_cb),
            LV_EVENT_CLICKED,
            core::ptr::null_mut(),
        );
        // Make the keypad-focus highlight unmistakable: the default theme's
        // focused style is too subtle on dark backgrounds, and a no-touch
        // 4-button device relies entirely on seeing which row ENTER will
        // activate. Fill the focused row with the accent color.
        let focus_sel = LV_PART_MAIN | LV_STATE_FOCUSED;
        lv_obj_set_style_bg_color(row, lv_color_hex(FOCUS_HIGHLIGHT_RGB), focus_sel);
        lv_obj_set_style_bg_opa(row, LV_OPA_COVER, focus_sel);
    }
}

/// `ListView.setOnItemClickListener` backing: register a Java `ListView` object
/// as the item-click target for the list handle. Mirrors
/// `spinner::register_listener` — update-in-place if already registered.
pub(in crate::system::picodroid::graphics) fn register_item_click_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        for entry in &mut LISTENER_MAP[..LISTENER_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if LISTENER_MAP_LEN < MAX_LIST_LISTENERS {
            LISTENER_MAP[LISTENER_MAP_LEN] = (raw_ptr, obj_ref);
            LISTENER_MAP_LEN += 1;
        }
    }
}

/// Drain one item-click event (raw row `lv_obj_t*`) from the queue.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_item_click_queue() -> Option<usize> {
    unsafe {
        if ITEM_CLICK_QUEUE_TAIL == ITEM_CLICK_QUEUE_HEAD {
            return None;
        }
        let row = ITEM_CLICK_QUEUE[ITEM_CLICK_QUEUE_TAIL];
        ITEM_CLICK_QUEUE_TAIL = (ITEM_CLICK_QUEUE_TAIL + 1) % ITEM_CLICK_QUEUE_SIZE;
        Some(row)
    }
}

/// Resolve a clicked row pointer to `(Java ListView object ref, item position)`.
/// Returns `None` if the row's parent list has no registered item-click
/// listener, or the row is no longer a child of its list. The position is the
/// row's index among the list's children, recovered by scan.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_item_click(row: usize) -> Option<(u16, i32)> {
    unsafe {
        let row_obj = row as *mut lv_obj_t;
        let list = lv_obj_get_parent(row_obj);
        if list.is_null() {
            return None;
        }
        let mut obj_ref = None;
        for &(list_ptr, r) in &LISTENER_MAP[..LISTENER_MAP_LEN] {
            if list_ptr == list as usize {
                obj_ref = Some(r);
                break;
            }
        }
        let obj_ref = obj_ref?;
        let n = lv_obj_get_child_count(list) as i32;
        for i in 0..n {
            if lv_obj_get_child(list, i) == row_obj {
                return Some((obj_ref, i));
            }
        }
        None
    }
}

pub fn reset_list_view_state() {
    unsafe {
        LISTENER_MAP_LEN = 0;
        ITEM_CLICK_QUEUE_HEAD = 0;
        ITEM_CLICK_QUEUE_TAIL = 0;
    }
}

/// Visit the Java `ListView` object ref of every list registered for an
/// item-click listener so the GC keeps it alive — a `ListView` referenced only
/// by this native map (the app kept no field for it) would otherwise be swept,
/// after which its item-clicks silently stop dispatching. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_item_click_listener_roots(visit: &mut dyn FnMut(u16)) {
    unsafe {
        for &(_, r) in &LISTENER_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
    }
}
