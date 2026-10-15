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

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::PtrMap;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

/// Accent fill (RGB888) for the keypad-focused list row — a material-teal that
/// reads clearly as "selected" on both light and dark surfaces. The default
/// theme's focus styling is too subtle to rely on for no-touch navigation.
const FOCUS_HIGHLIGHT_RGB: u32 = 0x0026_A69A;

// ── Item-click event queue (raw row `lv_obj_t*` pointers) ───────────────────

const ITEM_CLICK_QUEUE_SIZE: usize = 16;
struct State {
    item_click_queue: LocalRing<usize, ITEM_CLICK_QUEUE_SIZE>,
    listener_map: PtrMap<MAX_LIST_LISTENERS>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        item_click_queue: LocalRing::new(0),
        listener_map: PtrMap::new(),
    })
};

// ── ListView handle → Java object mapping (one entry per ListView) ──────────

const MAX_LIST_LISTENERS: usize = 16;

// Keyed by the LIST object; its rows' trampolines die with the rows, and this
// delete callback (attached at first registration) drops the map entry when
// the list itself is deleted.
lv_event_cb! {
    fn list_map_delete_cb(e) {
        let obj = e.target().addr();
        STATE.listener_map.remove(obj)
    }
}

lv_event_cb! {
    fn row_click_cb(e) {
        // The *current* target is the row the callback is registered on. An
        // adapter's row may be a layout whose child bubbles the click up
        // (LV_OBJ_FLAG_EVENT_BUBBLE); the original target would then be the
        // child, which is no child of the list and would never resolve.
        STATE.item_click_queue.push(e.current_target().addr());
    }
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
    let Some(row) = handle_table::get(row_id) else {
        return; // released between getView and here: nothing to style
    };
    lv_obj_set_width(row, lv_pct(100));
    lv_obj_set_style_pad_all(row, ROW_PAD, LV_PART_MAIN);
    lv_obj_set_style_border_width(row, 1, LV_PART_MAIN);
    lv_obj_set_style_border_color(row, lv_color_hex(ROW_SEPARATOR_RGB), LV_PART_MAIN);
    lv_obj_set_style_border_side(row, LV_BORDER_SIDE_BOTTOM, LV_PART_MAIN);
    // A label is not clickable by default; a layout is. Either way the
    // row itself must take the click and the keypad focus.
    lv_obj_add_flag(row, LV_OBJ_FLAG_CLICKABLE | LV_OBJ_FLAG_CLICK_FOCUSABLE);
    row_make_interactive(row);
}

/// The part of a row's setup shared by native text rows and adapter rows:
/// focus-group membership, the click trampoline and the focus highlight.
fn row_make_interactive(row: Obj) {
    {
        // Make the row keypad-traversable: join the active Activity focus
        // group. Idempotent if `lv_list_add_button` already auto-joined it, and
        // a no-op when no group is active (non-button boards, or before the
        // first Activity launches), matching `events::set_view_focusable`.
        if let Some(group) = lv_group_get_default() {
            lv_group_add_obj(group, row);
        }
        // Every row is clickable; the trampoline enqueues the row pointer. The
        // drain side no-ops if the row's list has no registered item listener.
        add_event_cb(row, row_click_cb, LV_EVENT_CLICKED);
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
    let Some(list) = lifecycle::screen_ptr().and_then(|o| lv_list_create(o)) else {
        return 0;
    };
    // A list scrolls vertically too; whether a step may use the panel is
    // decided per step (hw_scroll.rs), and a themed list with its border
    // will be refused until an app strips it.
    #[cfg(hw_vscroll)]
    super::super::hw_scroll::watch(list);
    handle_table::register(list)
}

pub(in crate::graphics) fn add_item(id: i32, text: &str) {
    let Some(list) = handle_table::get(id) else {
        return; // stale handle — mutating a destroyed View is a no-op
    };
    if let Some(row) = super::text_view::with_cstr(text, |p| list_add_button(list, p)) {
        row_make_interactive(row);
    }
}

/// `ListView.setOnItemClickListener` backing: register a Java `ListView` object
/// as the item-click target for the list handle. Mirrors
/// `spinner::register_listener` — update-in-place if already registered.
pub(in crate::graphics) fn register_item_click_listener(id: i32, obj_ref: u16) {
    // A deleted/stale list: never hand LVGL a null, never map key 0.
    super::compound::register_listener_with_delete_hook(
        &STATE.listener_map,
        id,
        obj_ref,
        list_map_delete_cb,
        "list-item-click",
    );
}

/// Drain one item-click event (raw row `lv_obj_t*`) from the queue.
pub fn drain_item_click_queue() -> Option<usize> {
    STATE.item_click_queue.pop()
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
    let mut hit = None;
    STATE.listener_map.for_each(&mut |list, obj_ref| {
        if hit.is_some() {
            return;
        }
        // SAFETY: a list is in the map from its first registration until
        // `list_map_delete_cb`, which LVGL runs before it frees the object.
        let Some(list) = (unsafe { Obj::from_raw(list as *mut lv_obj_t) }) else {
            return;
        };
        let n = lv_obj_get_child_count(list) as i32;
        for i in 0..n {
            if lv_obj_get_child(list, i).map(Obj::addr) == Some(row) {
                hit = Some((obj_ref, i));
                return;
            }
        }
    });
    hit
}

pub fn reset_list_view_state() {
    STATE.listener_map.reset();
    STATE.item_click_queue.clear();
}

/// Visit the Java `ListView` object ref of every list registered for an
/// item-click listener so the GC keeps it alive — a `ListView` referenced only
/// by this native map (the app kept no field for it) would otherwise be swept,
/// after which its item-clicks silently stop dispatching. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_item_click_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.listener_map.visit(visit)
}
