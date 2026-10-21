// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ListView` (LVGL `lv_list`).
//!
//! Rows are created with `lv_list_add_button` (a focusable, group-def,
//! clickable `lv_button`) rather than the non-focusable `lv_list_add_text`,
//! so on a hardware-button board the per-Activity keypad group can traverse
//! the rows (PREV/NEXT move the focus highlight) and ENTER activates the
//! focused row. Each row carries an `LV_EVENT_CLICKED` trampoline that
//! enqueues `(list handle, row slot)`; the main loop drains it and fires
//! `ListView.fireItemClick(slot)` on the Java `ListView` the handle table
//! binds to the list (`ListView.setOnItemClickListener`).
//!
//! Rows need no handle of their own (a text row is not a Java object): the
//! trampoline walks one step up to the list and reads the row's index among
//! its siblings at click time. The Java side turns that slot into an adapter
//! position: a long list keeps only a window of rows alive and re-binds them
//! as it scrolls (docs/designs/listview-recycling-2026-10.md), with the
//! list's own top and bottom padding standing in for the rows above and
//! below the window. This file owns the LVGL half of that: the scroll and
//! size events the window follows (queued here, drained by the framework
//! loop into `ListView.fireScroll`), the padding, a row's move from one end
//! of the window to the other, and the keypad group's order.

use crate::lvgl_ffi::*;

use super::super::focus_order::{self, GroupOps};
use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

/// Accent fill (RGB888) for the keypad-focused list row — a material-teal that
/// reads clearly as "selected" on both light and dark surfaces. The default
/// theme's focus styling is too subtle to rely on for no-touch navigation.
const FOCUS_HIGHLIGHT_RGB: u32 = 0x0026_A69A;

// ── Event queues: item clicks `(list handle, row slot)`, viewport moves ─────

const ITEM_CLICK_QUEUE_SIZE: usize = 16;
/// Lists whose viewport moved this frame. A handle is queued once per frame
/// (the scroll event fires per pixel step), and a dropped one is made up for
/// by the next step: few screens hold more than two lists.
const SCROLL_QUEUE_SIZE: usize = 8;

struct State {
    item_click_queue: LocalRing<(i32, i32), ITEM_CLICK_QUEUE_SIZE>,
    scroll_queue: LocalRing<i32, SCROLL_QUEUE_SIZE>,
    /// While set, a row gaining the focus does not scroll into view: the
    /// focus is being handed on or restored by the window's own bookkeeping,
    /// not moved by the user.
    hold_focus_scroll: Cell<bool>,
}

// SAFETY: filled by LVGL callbacks and by natives, drained by the framework
// loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        item_click_queue: LocalRing::new((0, 0)),
        scroll_queue: LocalRing::new(0),
        hold_focus_scroll: Cell::new(false),
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

lv_event_cb! {
    fn viewport_cb(e) {
        // Registered on the list for LV_EVENT_SCROLL and LV_EVENT_SIZE_CHANGED;
        // the current target is the list either way (a child's own scroll
        // does not bubble here). The JVM is unreachable inside a tick, so only
        // the handle is recorded, once per frame.
        let list = handle_table::handle_of(e.current_target());
        if list == 0 || STATE.scroll_queue.any(|h| h == list) {
            return;
        }
        STATE.scroll_queue.push(list);
    }
}

lv_event_cb! {
    fn row_focus_cb(e) {
        // A focused row scrolls into view, as LVGL's SCROLL_ON_FOCUS would
        // do, but without it: LVGL's scroll-into-view treats the parent's
        // padding as a viewport inset, and a recycling list's bottom padding
        // is the whole rest of the content, so one DOWN press would scroll
        // to the end of the list. The row's and the list's own coordinates
        // say what is in view.
        if STATE.hold_focus_scroll.get() {
            return;
        }
        let row = e.current_target();
        let Some(list) = lv_obj_get_parent(row) else {
            return;
        };
        scroll_row_into_view(list, row);
    }
}

/// Scroll `list` by the least that brings `row` fully into its box (the
/// border excepted), animated; nothing when it already is.
fn scroll_row_into_view(list: Obj, row: Obj) {
    let mut lr = lv_area_t {
        x1: 0,
        y1: 0,
        x2: 0,
        y2: 0,
    };
    let mut rr = lr;
    lv_obj_get_coords(list, &mut lr);
    lv_obj_get_coords(row, &mut rr);
    let dy = if rr.y1 < lr.y1 {
        rr.y1 - lr.y1
    } else if rr.y2 > lr.y2 {
        rr.y2 - lr.y2
    } else {
        return;
    };
    let y = lv_obj_get_scroll_y(list).saturating_add(dy);
    lv_obj_scroll_to(list, lv_obj_get_scroll_x(list), y, LV_ANIM_ON);
}

/// Separator under each row (RGB888): the default theme's grey list divider.
const ROW_SEPARATOR_RGB: u32 = 0x00BD_BDBD;
/// Row padding in pixels: the default theme's `PAD_SMALL` on a small panel.
const ROW_PAD: i32 = 10;

/// `ListView.nativeStyleRow`: make an adapter-built row view (already a child
/// of the list) behave like a row `lv_list_add_button` makes — stretched to
/// the list's width, padded, separated, clickable, keypad-focusable in the
/// Activity's group, highlighted when focused, and enqueueing the item click.
/// `height > 0` pins the row to the list's row pitch: a recycling list needs
/// every row the height of its first (the padding that stands in for the
/// rows outside the window is counted in rows).
pub(in crate::graphics) fn style_row(row_id: i32, height: i32) {
    let Some(row) = handle_table::get(row_id) else {
        return; // released between getView and here: nothing to style
    };
    lv_obj_set_width(row, lv_pct(100));
    if height > 0 {
        lv_obj_set_height(row, height);
    }
    lv_obj_set_style_pad_all(row, ROW_PAD, LV_PART_MAIN);
    lv_obj_set_style_border_width(row, 1, LV_PART_MAIN);
    lv_obj_set_style_border_color(row, lv_color_hex(ROW_SEPARATOR_RGB), LV_PART_MAIN);
    lv_obj_set_style_border_side(row, LV_BORDER_SIDE_BOTTOM, LV_PART_MAIN);
    // A label is not clickable by default; a layout is. Either way the
    // row itself must take the click and the keypad focus, and a focused
    // row must scroll into view: `lv_button` (the native text row) carries
    // SCROLL_ON_FOCUS from its class, a label or a layout does not, and
    // without a scroll DOWN past the last visible row wraps the group's
    // focus around the window instead of scrolling the next row in. The
    // scroll is this file's (`row_focus_cb`), not LVGL's: see there.
    lv_obj_add_flag(row, LV_OBJ_FLAG_CLICKABLE | LV_OBJ_FLAG_CLICK_FOCUSABLE);
    lv_obj_remove_flag(row, LV_OBJ_FLAG_SCROLL_ON_FOCUS);
    lv_obj_remove_event_cb(row, Some(row_focus_cb));
    add_event_cb(row, row_focus_cb, LV_EVENT_FOCUSED);
    row_make_interactive(row);
}

/// The part of a row's setup shared by native text rows and adapter rows:
/// focus-group membership, the click trampoline and the focus highlight.
/// Idempotent: a row styled twice must not click twice.
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
        lv_obj_remove_event_cb(row, Some(row_click_cb));
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
    // The default theme's list has no vertical padding and no gap between
    // rows (`list_bg`: pad_ver 0, pad_gap 0). Pin that here rather than rely
    // on it: a recycling list counts the padding that stands in for the rows
    // outside its window in row heights, so a row's pitch must be exactly its
    // height, and `set_spacers` overwrites the top and bottom padding anyway.
    lv_obj_set_style_pad_row(list, 0, LV_PART_MAIN);
    lv_obj_set_style_pad_top(list, 0, LV_PART_MAIN);
    lv_obj_set_style_pad_bottom(list, 0, LV_PART_MAIN);
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

// ── The window: events, padding, moves, focus order ─────────────────────────

/// `ListView.nativeWatchViewport`: queue the list for `fireScroll` whenever
/// it scrolls or changes size. Idempotent; a no-op for a released view. Only
/// a list long enough to recycle asks for this, so a short list pays nothing.
pub(in crate::graphics) fn watch_viewport(id: i32) {
    let Some(list) = handle_table::get(id) else {
        return;
    };
    lv_obj_remove_event_cb(list, Some(viewport_cb));
    add_event_cb(list, viewport_cb, LV_EVENT_SCROLL);
    add_event_cb(list, viewport_cb, LV_EVENT_SIZE_CHANGED);
}

/// `ListView.nativeScrollY`: how far the content has scrolled up, in pixels.
pub(in crate::graphics) fn scroll_y(id: i32) -> i32 {
    handle_table::get(id).map_or(0, |list| lv_obj_get_scroll_y(list))
}

/// `ListView.nativeScrollTo`: scroll so content row `y` is at the top,
/// clamped by LVGL to the content; animated for `smoothScrollToPosition`.
pub(in crate::graphics) fn scroll_to_y(id: i32, y: i32, animate: bool) {
    let Some(list) = handle_table::get(id) else {
        return;
    };
    let anim = if animate { LV_ANIM_ON } else { LV_ANIM_OFF };
    lv_obj_scroll_to(list, lv_obj_get_scroll_x(list), y, anim);
}

/// `ListView.nativeSetSpacers`: the padding that stands in for the rows
/// above and below the window, in pixels. Zero both when the list is short
/// enough to hold every row.
pub(in crate::graphics) fn set_spacers(id: i32, top: i32, bottom: i32) {
    let Some(list) = handle_table::get(id) else {
        return;
    };
    lv_obj_set_style_pad_top(list, top.max(0), LV_PART_MAIN);
    lv_obj_set_style_pad_bottom(list, bottom.max(0), LV_PART_MAIN);
}

/// `ListView.nativeRecycleRow`: move a row from one end of the window to the
/// other (`to_end`: from the top to the bottom), ahead of its re-binding. If
/// the row holds the keypad focus (a touch scroll pushed the focused row
/// out), the focus moves to its neighbour first, without the scroll into
/// view a focus normally brings: the neighbour is at the window's edge and
/// in view already, and a scroll of ours would fight the one in progress.
pub(in crate::graphics) fn recycle_row(row_id: i32, to_end: bool) {
    let Some(row) = handle_table::get(row_id) else {
        return;
    };
    let Some(list) = lv_obj_get_parent(row) else {
        return;
    };
    if let Some(group) = lv_obj_get_group(row) {
        if lv_group_get_focused(group) == Some(row) {
            let idx = lv_obj_get_index(row);
            let neighbour = if to_end { idx + 1 } else { idx - 1 };
            if let Some(n) = (neighbour >= 0)
                .then(|| lv_obj_get_child(list, neighbour))
                .flatten()
            {
                STATE.hold_focus_scroll.set(true);
                lv_group_focus_obj(n);
                STATE.hold_focus_scroll.set(false);
            }
        }
    }
    lv_obj_move_to_index(row, if to_end { -1 } else { 0 });
}

/// The most rows a window is expected to hold: a 480-row panel with 24-pixel
/// rows is 23 plus the overscan. A longer window keeps its focus order
/// unsynchronised rather than overflow the buffer; the Java side never asks
/// for one on a panel this project targets.
const MAX_WINDOW_ROWS: usize = 32;

/// The Activity's keypad group, as `focus_order::sync` sees it.
struct LvGroup(Group);

impl GroupOps for LvGroup {
    type Id = Obj;

    fn count(&self) -> u32 {
        lv_group_get_obj_count(self.0)
    }

    fn at(&self, index: u32) -> Option<Obj> {
        lv_group_get_obj_by_index(self.0, index)
    }

    fn swap(&mut self, a: Obj, b: Obj) {
        lv_group_swap_obj(a, b);
    }
}

/// `ListView.nativeSyncFocusOrder`: make the keypad group's run of rows
/// follow the list's child order, after rows moved or were replaced. A
/// no-op without a group (a touch board) or for an empty list.
pub(in crate::graphics) fn sync_focus_order(id: i32) {
    let Some(list) = handle_table::get(id) else {
        return;
    };
    let count = lv_obj_get_child_count(list) as usize;
    if count == 0 || count > MAX_WINDOW_ROWS {
        return;
    }
    // Any row in a group names the group; the first row is the usual one.
    let Some(group) = (0..count)
        .filter_map(|i| lv_obj_get_child(list, i as i32))
        .find_map(|row| lv_obj_get_group(row))
    else {
        return;
    };
    // The list itself is only a filler for the slots past `count`.
    let mut rows = [list; MAX_WINDOW_ROWS];
    let mut n = 0;
    for (slot, row) in rows.iter_mut().enumerate().take(count) {
        if let Some(child) = lv_obj_get_child(list, slot as i32) {
            *row = child;
            n += 1;
        }
    }
    let rows = &rows[..n];
    // `lv_group_swap_obj` swaps the focus along with the positions: a swap
    // that touches the focused row hands the focus to its partner, which
    // would scroll that row into view, slide the window, swap again, and walk
    // the focus down the whole list from one key press. So the focus is held
    // across the sync: a focus hop meanwhile scrolls nothing, and the object
    // that was focused is focused again at the end.
    let focused = lv_group_get_focused(group);
    STATE.hold_focus_scroll.set(true);
    focus_order::sync(rows, &mut LvGroup(group));
    if let Some(f) = focused {
        if lv_group_get_focused(group) != Some(f) {
            lv_group_focus_obj(f);
        }
    }
    STATE.hold_focus_scroll.set(false);
}

/// Drain one item click: `(list handle, row slot)`.
pub fn drain_item_click_queue() -> Option<(i32, i32)> {
    STATE.item_click_queue.pop()
}

/// Drain one list whose viewport moved since the last frame.
pub fn drain_scroll_queue() -> Option<i32> {
    STATE.scroll_queue.pop()
}

pub fn reset_list_view_state() {
    STATE.item_click_queue.clear();
    STATE.scroll_queue.clear();
}
