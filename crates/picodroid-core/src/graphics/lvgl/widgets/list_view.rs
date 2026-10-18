// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ListView` (LVGL `lv_list`).
//!
//! Rows are created with `lv_list_add_button` (a focusable, group-def,
//! clickable `lv_button`) rather than the non-focusable `lv_list_add_text`,
//! so on a hardware-button board the per-Activity keypad group can traverse
//! the rows (PREV/NEXT move the focus highlight) and ENTER activates the
//! focused row. Each row carries an `LV_EVENT_CLICKED` trampoline that
//! enqueues `(list handle, row position)`; the main loop drains it and fires
//! `ListView.fireItemClick(position)` on the Java `ListView` the handle
//! table binds to the list (`ListView.setOnItemClickListener`).
//!
//! Rows need no handle of their own (a text row is not a Java object): the
//! trampoline walks one step up to the list and reads the row's index among
//! its siblings at click time, which is also the position Android reports.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

/// Accent fill (RGB888) for the keypad-focused list row — a material-teal that
/// reads clearly as "selected" on both light and dark surfaces. The default
/// theme's focus styling is too subtle to rely on for no-touch navigation.
const FOCUS_HIGHLIGHT_RGB: u32 = 0x0026_A69A;

// ── Item-click event queue: `(list handle, row position)` ───────────────────

const ITEM_CLICK_QUEUE_SIZE: usize = 16;
struct State {
    item_click_queue: LocalRing<(i32, i32), ITEM_CLICK_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        item_click_queue: LocalRing::new((0, 0)),
    })
};

lv_event_cb! {
    fn row_click_cb(e) {
        // The *current* target is the row the callback is registered on. An
        // adapter's row may be a layout whose child bubbles the click up
        // (LV_OBJ_FLAG_EVENT_BUBBLE); the original target would then be the
        // child, which is no child of the list and would never resolve.
        let row = e.current_target();
        let Some(list) = lv_obj_get_parent(row) else {
            return;
        };
        let list_handle = handle_table::handle_of(list);
        if list_handle != 0 {
            STATE
                .item_click_queue
                .push((list_handle, lv_obj_get_index(row)));
        }
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

/// Drain one item click: `(list handle, row position)`.
pub fn drain_item_click_queue() -> Option<(i32, i32)> {
    STATE.item_click_queue.pop()
}

pub fn reset_list_view_state() {
    STATE.item_click_queue.clear();
}
