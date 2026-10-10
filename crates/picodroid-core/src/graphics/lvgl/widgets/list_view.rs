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
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

/// Accent fill (RGB888) for the keypad-focused list row — a material-teal that
/// reads clearly as "selected" on both light and dark surfaces. The default
/// theme's focus styling is too subtle to rely on for no-touch navigation.
const FOCUS_HIGHLIGHT_RGB: u32 = 0x0026_A69A;

// ── Item-click event queue (raw row `lv_obj_t*` pointers) ───────────────────

const ITEM_CLICK_QUEUE_SIZE: usize = 16;
// SAFETY: filled by LVGL callbacks and drained by natives, all on JVM tasks.
static ITEM_CLICK_QUEUE: Core0<LocalRing<usize, ITEM_CLICK_QUEUE_SIZE>> =
    unsafe { Core0::new(LocalRing::new(0)) };

// ── ListView handle → Java object mapping (one entry per ListView) ──────────

const MAX_LIST_LISTENERS: usize = 16;
// SAFETY: a listener registry, reached only from JVM tasks.
static LISTENER_MAP: Core0<PtrMap<MAX_LIST_LISTENERS>> = unsafe { Core0::new(PtrMap::new()) };

// Keyed by the LIST object; its rows' trampolines die with the rows, and this
// delete callback (attached at first registration) drops the map entry when
// the list itself is deleted.
unsafe extern "C" fn list_map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    LISTENER_MAP.remove(obj)
}

unsafe extern "C" fn row_click_cb(e: *mut lv_event_t) {
    // The *current* target is the row the callback is registered on. An
    // adapter's row may be a layout whose child bubbles the click up
    // (LV_OBJ_FLAG_EVENT_BUBBLE); the original target would then be the
    // child, which is no child of the list and would never resolve.
    let row = unsafe { lv_event_get_current_target_obj(e) };
    ITEM_CLICK_QUEUE.push(row as usize);
}

/// Separator under each row (RGB888): the default theme's grey list divider.
const ROW_SEPARATOR_RGB: u32 = 0x00BD_BDBD;
/// Row padding in pixels: the default theme's `PAD_SMALL` on a small panel.
const ROW_PAD: i32 = 10;

/// `ListView.nativeStyleRow`: make an adapter-built row view (already a child
/// of the list) behave like a row `lv_list_add_button` makes — stretched to
/// the list's width, padded, separated, clickable, keypad-focusable in the
/// Activity's group, highlighted when focused, and enqueueing the item click.
pub(in crate::graphics) fn style_row(row_id: i32) {
    let row = handle_table::lookup(row_id);
    if row.is_null() {
        return; // released between getView and here: nothing to style
    }
    // SAFETY: `row` is a live object the handle table just resolved, on the
    // JVM task that owns LVGL; the style setters only write its style
    // properties and flags.
    unsafe {
        lv_obj_set_width(row, lv_pct(100));
        lv_obj_set_style_pad_all(row, ROW_PAD, LV_PART_MAIN);
        lv_obj_set_style_border_width(row, 1, LV_PART_MAIN);
        lv_obj_set_style_border_color(row, lv_color_hex(ROW_SEPARATOR_RGB), LV_PART_MAIN);
        lv_obj_set_style_border_side(row, LV_BORDER_SIDE_BOTTOM, LV_PART_MAIN);
        // A label is not clickable by default; a layout is. Either way the
        // row itself must take the click and the keypad focus.
        lv_obj_add_flag(row, LV_OBJ_FLAG_CLICKABLE | LV_OBJ_FLAG_CLICK_FOCUSABLE);
    }
    row_make_interactive(row);
}

/// The part of a row's setup shared by native text rows and adapter rows:
/// focus-group membership, the click trampoline and the focus highlight.
/// `row` must be a live, non-null `lv_obj_t` (both callers just created or
/// resolved it).
fn row_make_interactive(row: *mut lv_obj_t) {
    // SAFETY: `row` is live and non-null (see above) and this runs on the
    // JVM task that owns LVGL. The group pointer is checked before use, and
    // the callback registered takes no user data.
    unsafe {
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
        // activate. Fill the focused row with the accent color. Cover BOTH
        // LV_STATE_FOCUSED and LV_STATE_FOCUS_KEY: keypad navigation adds
        // FOCUS_KEY, and without overriding it the default theme repaints the
        // row blue after the first move (teal on first render, blue after).
        for state in [LV_STATE_FOCUSED, LV_STATE_FOCUS_KEY] {
            let sel = LV_PART_MAIN | state;
            lv_obj_set_style_bg_color(row, lv_color_hex(FOCUS_HIGHLIGHT_RGB), sel);
            lv_obj_set_style_bg_opa(row, LV_OPA_COVER, sel);
        }
    }
}

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_list_create(lifecycle::screen_ptr()) };
    // A list scrolls vertically too; whether a step may use the panel is
    // decided per step (hw_scroll.rs), and a themed list with its border
    // will be refused until an app strips it.
    #[cfg(hw_vscroll)]
    super::super::hw_scroll::watch(ptr);
    handle_table::register(ptr)
}

pub(in crate::graphics) fn add_item(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe {
        let list = handle_table::lookup(id);
        let row = lv_list_add_button(list, core::ptr::null(), buf.as_ptr() as *const c_char);
        row_make_interactive(row);
    }
}

/// `ListView.setOnItemClickListener` backing: register a Java `ListView` object
/// as the item-click target for the list handle. Mirrors
/// `spinner::register_listener` — update-in-place if already registered.
pub(in crate::graphics) fn register_item_click_listener(id: i32, obj_ref: u16) {
    let raw_obj = handle_table::lookup(id);
    if raw_obj.is_null() {
        return; // deleted/stale list: never hand LVGL a null, never map key 0
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        match LISTENER_MAP.upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("list-item-click"),
            Upsert::Inserted => {
                lv_obj_add_event_cb(
                    raw_ptr as *mut lv_obj_t,
                    Some(list_map_delete_cb),
                    LV_EVENT_DELETE,
                    core::ptr::null_mut(),
                );
            }
        }
    }
}

/// Drain one item-click event (raw row `lv_obj_t*`) from the queue.
pub fn drain_item_click_queue() -> Option<usize> {
    ITEM_CLICK_QUEUE.pop()
}

/// Resolve a clicked row pointer to `(Java ListView object ref, item position)`.
/// Returns `None` if the row's parent list has no registered item-click
/// listener, or the row is no longer a child of its list. The position is the
/// row's index among the list's children, recovered by scan.
pub fn lookup_item_click(row: usize) -> Option<(u16, i32)> {
    // The queued row pointer is never dereferenced: an earlier click in the
    // same drain may have rebuilt the list (removeAllViews / a
    // notifyDataSetChanged-style repopulate) or finished the Activity, and
    // this queue has no LV_EVENT_DELETE purge. Instead, walk the children of
    // every list that still has a listener (those pointers are kept live by
    // list_map_delete_cb) and match the row by address — a freed row is
    // simply not found.
    let row_obj = row as *mut lv_obj_t;
    let mut hit = None;
    unsafe {
        LISTENER_MAP.for_each(&mut |list, obj_ref| {
            if hit.is_some() {
                return;
            }
            let list = list as *mut lv_obj_t;
            let n = lv_obj_get_child_count(list) as i32;
            for i in 0..n {
                if lv_obj_get_child(list, i) == row_obj {
                    hit = Some((obj_ref, i));
                    return;
                }
            }
        });
    }
    hit
}

pub fn reset_list_view_state() {
    LISTENER_MAP.reset();
    ITEM_CLICK_QUEUE.clear();
}

/// Visit the Java `ListView` object ref of every list registered for an
/// item-click listener so the GC keeps it alive — a `ListView` referenced only
/// by this native map (the app kept no field for it) would otherwise be swept,
/// after which its item-clicks silently stop dispatching. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_item_click_listener_roots(visit: &mut dyn FnMut(u16)) {
    LISTENER_MAP.visit(visit)
}
