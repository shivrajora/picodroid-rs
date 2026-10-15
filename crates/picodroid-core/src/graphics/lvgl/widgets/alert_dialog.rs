// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `AlertDialog` — a modal dialog with title/message and up to
//! three buttons (neutral / negative / positive, left to right — Android's
//! placement).
//!
//! Composition (z-order, bottom→top):
//! - `scrim` — fullscreen `lv_obj_t` parented to the active screen, with
//!   `LV_OBJ_FLAG_CLICKABLE` set so it absorbs taps that fall outside the
//!   card. This is what makes the dialog modal: clicks on widgets behind
//!   the dialog never propagate.
//! - `card` — centered child of `scrim`, containing the title label,
//!   message label, and a button row.
//!
//! Button clicks feed a small static ring buffer keyed by the dialog's
//! root scrim handle plus a `which` discriminator (0 = positive, 1 =
//! negative). The framework event loop drains the queue and invokes
//! `AlertDialog.fireButtonClick(int which)` on the matching Java object.

use crate::lvgl_ffi::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::Cell;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

const BUTTON_POSITIVE: i32 = 0;
const BUTTON_NEGATIVE: i32 = 1;
const BUTTON_NEUTRAL: i32 = 2;

/// List render mode, matching the Java `AlertDialog` constants passed to
/// `nativeCreateWithList`.
const LIST_MODE_ITEMS: i32 = 0;
const LIST_MODE_SINGLE: i32 = 1;
const LIST_MODE_MULTI: i32 = 2;

/// Hard cap on list items — focusable rows beyond ~12 stall the 48 KB LVGL
/// renderer. A button-matrix is one object so it is far safer than lv_list,
/// but the cap stays as documented defence-in-depth; the Java side enforces
/// it too (throws IllegalArgumentException) so this is a silent clamp.
const MAX_LIST_ITEMS: usize = 12;

// ── Click event queue (ring buffer) ─────────────────────────────────────────

const CLICK_QUEUE_SIZE: usize = 8;

#[derive(Copy, Clone)]
struct ClickRecord {
    /// Raw `lv_obj_t*` of the dialog's scrim — used to look up the Java
    /// AlertDialog object.
    dialog_handle: usize,
    /// 0 = positive, 1 = negative.
    which: i32,
}

const EMPTY_CLICK: ClickRecord = ClickRecord {
    dialog_handle: 0,
    which: 0,
};

// ── Item-click event queue (list dialogs) ───────────────────────────────────

#[derive(Copy, Clone)]
struct ItemRecord {
    dialog_handle: usize,
    position: i32,
    /// For single/multi-choice lists: the post-click checked state of the
    /// row. For plain item lists this is always `true` (a click is a select).
    checked: bool,
}

const EMPTY_ITEM: ItemRecord = ItemRecord {
    dialog_handle: 0,
    position: 0,
    checked: false,
};

// ── List storage (owned button-matrix maps) ─────────────────────────────────
//
// LVGL's `lv_buttonmatrix_set_map` stores the `*const *const c_char` pointer
// it is handed: both the map array AND every string it points at must outlive
// the widget. We own that storage here, keyed by the dialog's scrim handle,
// and drop it when the dialog is dismissed. Separators are `'static` (`"\n"`
// row break, `""` map terminator), so only the item strings are heap-owned.

const NEWLINE_SEP: &[u8] = b"\n\0";
const MAP_TERMINATOR: &[u8] = b"\0";

const MAX_LIST_DIALOGS: usize = 4;

#[derive(Copy, Clone)]
struct ListSlot {
    dialog_handle: usize,
    /// The matrix object — its mode controls how clicks are interpreted.
    matrix: usize,
    mode: i32,
}

const EMPTY_LIST_SLOT: ListSlot = ListSlot {
    dialog_handle: 0,
    matrix: 0,
    mode: LIST_MODE_ITEMS,
};

/// What a list slot's matrix points into, kept alive with the slot.
struct ListStorage {
    /// Owned NUL-terminated item strings; addresses must stay stable while
    /// the matrix lives, so each is its own boxed slice.
    items: Vec<Box<[u8]>>,
    /// The map array LVGL points at: item ptr, "\n", item ptr, …, "". Its
    /// heap buffer stays put however the `Vec` itself moves.
    _map: Vec<*const c_char>,
}

// ── Button → dialog mapping (each LVGL button knows its parent dialog) ──────
//
// Three entries per dialog (positive, negative, neutral). Stored as
// (button_ptr, dialog_ptr, which). Sized to the dialog table: at 16 the
// map filled under eight three-button dialogs and `register_button`
// returned in silence, so the button never dispatched.

const MAX_BUTTONS: usize = MAX_DIALOGS * 3;

#[derive(Copy, Clone)]
struct ButtonEntry {
    button_handle: usize,
    dialog_handle: usize,
    which: i32,
}

const EMPTY_BUTTON: ButtonEntry = ButtonEntry {
    button_handle: 0,
    dialog_handle: 0,
    which: 0,
};

// ── Dialog → Java object mapping (for click dispatch) ───────────────────────

const MAX_DIALOGS: usize = 8;

lv_event_cb! {
    /// Belt-and-braces unregistration for any path that deletes the scrim without
    /// going through [`dismiss`] (which already calls `unregister_dialog`).
    /// Attached to every scrim at create; double removal is a no-op.
    fn scrim_delete_cb(e) {
        let obj = e.target().addr();
        unregister_dialog(obj);
        unregister_list(obj);
    }
}

// ── Shown-dialog stack ──────────────────────────────────────────────────────
//
// Java `nativeHandle` ids of dialogs currently on screen, newest on top. Lets
// the framework dismiss the topmost dialog on BACK (Android's cancelable
// default) and tear down any dialog an Activity leaves up when it finishes —
// the scrim is parented to the screen, so it would otherwise outlive the
// Activity and leak onto the one beneath as an input-absorbing modal.

const MAX_SHOWN: usize = 8;

struct State {
    click_queue: LocalRing<ClickRecord, CLICK_QUEUE_SIZE>,
    item_queue: LocalRing<ItemRecord, CLICK_QUEUE_SIZE>,
    list_slots: [Cell<ListSlot>; MAX_LIST_DIALOGS],
    /// Map of `(matrix ptr → dialog scrim ptr)` so the VALUE_CHANGED
    /// trampoline can find the owning dialog (and its list slot) from the
    /// fired matrix.
    matrix_map: [Cell<(usize, usize)>; MAX_LIST_DIALOGS],
    button_map: [Cell<ButtonEntry>; MAX_BUTTONS],
    dialog_obj_map: PtrMap<MAX_DIALOGS>,
    /// The shown-dialog stack: `shown[..shown_len]`, oldest first.
    shown: [Cell<i32>; MAX_SHOWN],
    shown_len: Cell<usize>,
    /// Dialogs the framework dismissed, for Java (see below).
    dismiss_queue: LocalRing<u16, MAX_SHOWN>,
}

// SAFETY: built and dismissed by natives, filled by LVGL's click callbacks,
// drained by the framework loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        click_queue: LocalRing::new(EMPTY_CLICK),
        item_queue: LocalRing::new(EMPTY_ITEM),
        list_slots: [const { Cell::new(EMPTY_LIST_SLOT) }; MAX_LIST_DIALOGS],
        matrix_map: [const { Cell::new((0, 0)) }; MAX_LIST_DIALOGS],
        button_map: [const { Cell::new(EMPTY_BUTTON) }; MAX_BUTTONS],
        dialog_obj_map: PtrMap::new(),
        shown: [const { Cell::new(0) }; MAX_SHOWN],
        shown_len: Cell::new(0),
        dismiss_queue: LocalRing::new(0),
    })
};

/// `STATE.list_slots[i]`'s strings and map, `None` while the slot is free.
/// A static of its own: an empty `Vec` is not all-zero bytes, and kept apart
/// it leaves the rest of the dialog state in .bss.
// SAFETY: written by the list-dialog natives and the scrim's delete
// callback, all on core-0 JVM tasks.
static LIST_STORAGE: Core0<[Cell<Option<ListStorage>>; MAX_LIST_DIALOGS]> =
    unsafe { Core0::new([const { Cell::new(None) }; MAX_LIST_DIALOGS]) };

/// Free list slot `i`'s strings and map. Not inlined: the drop is the bulk
/// of it, and the callers loop over the slots.
#[inline(never)]
fn free_list_storage(i: usize) {
    LIST_STORAGE[i].set(None);
}

/// Track `id` as the topmost shown dialog. Returns the oldest dialog's id
/// when the table was full — the caller must dismiss it. An untracked
/// dialog is worse than a dismissed one: BACK and Activity teardown only
/// reach tracked ids, so a dropped entry left an input-absorbing scrim
/// alive across Activities (the AlertDialog-leak symptom past the cap).
fn shown_push(id: i32) -> Option<i32> {
    shown_remove(id); // de-dup so re-show moves it to the top
    let evicted = if STATE.shown_len.get() == MAX_SHOWN {
        let oldest = STATE.shown[0].get();
        shown_remove(oldest);
        Some(oldest)
    } else {
        None
    };
    let len = STATE.shown_len.get();
    STATE.shown[len].set(id);
    STATE.shown_len.set(len + 1);
    evicted
}

#[inline(never)]
fn shown_remove(id: i32) {
    let len = STATE.shown_len.get();
    let s = &STATE.shown[..len];
    if let Some(pos) = s.iter().position(|x| x.get() == id) {
        for pair in s[pos..].windows(2) {
            pair[0].set(pair[1].get()); // left-shift over the hole
        }
        s[len - 1].set(0);
        STATE.shown_len.set(len - 1);
    }
}

/// True if any dialog is currently on screen.
pub fn has_shown_dialog() -> bool {
    STATE.shown_len.get() > 0
}

/// The Java handle id of the most-recently-shown dialog, if any is showing.
fn topmost_shown_id() -> Option<i32> {
    match STATE.shown_len.get() {
        0 => None,
        len => Some(STATE.shown[len - 1].get()),
    }
}

/// Dismiss the most-recently-shown dialog without telling its Java object:
/// an Activity's teardown, where nothing is left to listen. Returns `true`
/// if one was dismissed, `false` if none were showing.
pub fn dismiss_topmost_dialog() -> bool {
    let Some(id) = topmost_shown_id() else {
        return false;
    };
    dismiss(id); // also pops it from SHOWN
    true
}

// ── Dialogs the framework dismissed, for Java ───────────────────────────────
//
// BACK tears the topmost dialog down here, natively, before Java hears of
// it. The Java `AlertDialog` is told afterwards (`fireDismiss`, drained by
// `lifecycle::dispatch_alert_dialog_dismissals`) so its `OnDismissListener`
// runs and `isShowing()` turns false — the options menu learns it closed
// from that; it used to stay "showing" and the next MENU closed it again
// (QA round 2, R1). The object refs wait here until the drain and are GC
// roots meanwhile: the map entry that rooted them went with the scrim.

/// BACK: dismiss the most-recently-shown dialog and queue its Java object
/// for `fireDismiss`. Returns `true` if one was dismissed, `false` if none
/// were showing.
pub fn cancel_topmost_dialog() -> bool {
    let Some(id) = topmost_shown_id() else {
        return false;
    };
    let obj_ref = if handle_table::is_live(id) {
        STATE
            .dialog_obj_map
            .lookup(handle_table::get(id).map_or(0, Obj::addr))
    } else {
        None
    };
    dismiss(id); // also pops it from SHOWN
    if let Some(obj_ref) = obj_ref {
        if !STATE.dismiss_queue.push(obj_ref) {
            crate::pd_warn!("AlertDialog: dismiss queue full, a listener is skipped");
        }
    }
    true
}

/// Drain one framework-dismissed dialog: the Java object to call
/// `fireDismiss` on.
pub fn drain_dismiss_queue() -> Option<u16> {
    STATE.dismiss_queue.pop()
}

// ── LVGL trampoline ─────────────────────────────────────────────────────────

lv_event_cb! {
    fn dialog_button_click_cb(e) {
        let btn = e.target().addr();
        // Look up which dialog this button belongs to.
        let Some(entry) = STATE
            .button_map
            .iter()
            .map(Cell::get)
            .find(|entry| entry.button_handle == btn)
        else {
            return;
        };
        STATE.click_queue.push(ClickRecord {
            dialog_handle: entry.dialog_handle,
            which: entry.which,
        });
    }
}

lv_event_cb! {
    /// VALUE_CHANGED trampoline for a list dialog's button-matrix. Enqueues
    /// the selected row index plus (for choice lists) its post-click checked
    /// state.
    fn dialog_item_click_cb(e) {
        // The matrix this callback is on (`create_with_list` registered it
        // there).
        let matrix = e.current_target();
        let dialog_handle = STATE
            .matrix_map
            .iter()
            .map(Cell::get)
            .find(|&(m, _)| m == matrix.addr())
            .map_or(0, |(_, d)| d);
        if dialog_handle == 0 {
            return;
        }
        // SAFETY: `matrix` is in `matrix_map`, so `create_with_list` made it
        // with `lv_buttonmatrix_create`.
        let sel = unsafe { lv_buttonmatrix_get_selected_button(matrix) };
        if sel == LV_BUTTONMATRIX_BUTTON_NONE {
            return;
        }
        // For checkable (choice) lists, report the row's post-click checked
        // state; for plain item lists every click is a select (true).
        // SAFETY: as above.
        let checked =
            unsafe { lv_buttonmatrix_has_button_ctrl(matrix, sel, LV_BUTTONMATRIX_CTRL_CHECKED) };

        STATE.item_queue.push(ItemRecord {
            dialog_handle,
            position: sel as i32,
            checked,
        });
    }
}

// ── LVGL ops (called from widgets/alert_dialog.rs Java shim) ────────────────

/// The card's width for a window `w` pixels wide: five eighths of it, never
/// narrower than the 200 px every 240- and 320-wide panel has always had
/// (so no tap coordinate moves there; `scripts/board-lib.sh::settings_dialog_ok`
/// aims at that card) and never wider than 360 on a wide panel.
fn dialog_card_width(w: i32) -> i32 {
    (w * 5 / 8).clamp(200, 360).min(w)
}

/// What fits inside the card: its width less the 12 px padding each side.
/// The button row and a list dialog's matrix span it (176 on the 200 card).
fn dialog_content_width() -> i32 {
    dialog_card_width(lifecycle::window_size().0) - 24
}

/// Build the modal scrim + card and add the title/message labels. Returns
/// `(scrim, card)`; the caller appends any list matrix and then the button
/// row (flex-column order = creation order). `card` height defaults to 160
/// but is overridden by list dialogs to fit the matrix.
fn build_dialog_shell(title: &str, message: &str, card_h: i32) -> Option<(Obj, Obj)> {
    // On the overlay layer, above the screen: a dialog must not pan with an
    // oversized content root, and it belongs to the Activity, not to the
    // root a `setContentView` replaces (the shown-dialog stack ties its life
    // to the Activity's). See `lifecycle::overlay_layer`.
    let scrim = lifecycle::overlay_layer().and_then(|o| lv_obj_create(o))?;
    add_event_cb(scrim, scrim_delete_cb, LV_EVENT_DELETE);

    // Modal scrim: the whole window, dim, click-absorbing. The whole window,
    // not a 240 px square: on the 320-wide testbench a tap beside the scrim
    // used to reach the rows behind the dialog and open a second one.
    let (screen_w, screen_h) = lifecycle::window_size();
    let card_w = dialog_card_width(screen_w);
    lv_obj_add_flag(scrim, LV_OBJ_FLAG_HIDDEN);
    lv_obj_add_flag(scrim, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_set_pos(scrim, 0, 0);
    lv_obj_set_size(scrim, screen_w, screen_h);
    lv_obj_set_style_bg_color(scrim, lv_color_hex(0x000000), 0);
    lv_obj_set_style_bg_opa(scrim, 160, 0);
    lv_obj_set_style_pad_left(scrim, 0, 0);
    lv_obj_set_style_pad_right(scrim, 0, 0);
    lv_obj_set_style_pad_top(scrim, 0, 0);
    lv_obj_set_style_pad_bottom(scrim, 0, 0);

    // Card: vertical flex with title, message, and a button row.
    let card = lv_obj_create(scrim)?;
    lv_obj_set_size(card, card_w, card_h);
    // Center the card in the window. Vertical offset keeps the card
    // centered as it grows for list content.
    let card_y = ((screen_h - card_h) / 2).max(8);
    lv_obj_set_pos(card, ((screen_w - card_w) / 2).max(0), card_y);
    lv_obj_set_style_bg_color(card, lv_color_hex(0xFFFFFF), 0);
    lv_obj_set_style_bg_opa(card, LV_OPA_COVER, 0);
    // The card's content is laid out to fit it; without this LVGL still
    // draws a scrollbar along its bottom edge when the button row spans
    // the content width exactly.
    lv_obj_remove_flag(card, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_set_style_pad_left(card, 12, 0);
    lv_obj_set_style_pad_right(card, 12, 0);
    lv_obj_set_style_pad_top(card, 12, 0);
    lv_obj_set_style_pad_bottom(card, 12, 0);
    lv_obj_set_flex_flow(card, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_align(
        card,
        LV_FLEX_ALIGN_START,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_START,
    );

    if !title.is_empty() {
        if let Some(title_label) = add_label(card, title) {
            lv_obj_set_style_text_color(title_label, lv_color_hex(0x000000), 0);
        }
    }

    if !message.is_empty() {
        if let Some(msg_label) = add_label(card, message) {
            lv_obj_set_style_text_color(msg_label, lv_color_hex(0x303030), 0);
        }
    }

    Some((scrim, card))
}

/// Append the title/cancel button row to `card`. Empty strings suppress the
/// corresponding button. Neutral is leftmost, then negative, then positive
/// (Android placement).
fn add_button_row(
    card: Obj,
    scrim_ptr: usize,
    positive_text: &str,
    negative_text: &str,
    neutral_text: &str,
) {
    let Some(btn_row) = lv_obj_create(card) else {
        return;
    };
    lv_obj_set_size(btn_row, dialog_content_width(), 50);
    lv_obj_set_style_bg_opa(btn_row, 0, 0);
    // A transparent row, not a boxed one: the theme gives every plain
    // object a border, which drew a frame around the two buttons.
    lv_obj_set_style_border_width(btn_row, 0, 0);
    lv_obj_set_style_pad_left(btn_row, 0, 0);
    lv_obj_set_style_pad_right(btn_row, 0, 0);
    lv_obj_set_style_pad_top(btn_row, 6, 0);
    lv_obj_set_style_pad_bottom(btn_row, 0, 0);
    lv_obj_set_flex_flow(btn_row, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(
        btn_row,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_CENTER,
    );

    // Three buttons share the 176px row (on the narrowest card) only at a
    // narrower width; with one or two, keep the roomier 80px Android-ish
    // buttons.
    let n_buttons = (!positive_text.is_empty()) as i32
        + (!negative_text.is_empty()) as i32
        + (!neutral_text.is_empty()) as i32;
    let btn_w = if n_buttons >= 3 { 54 } else { 80 };

    let add = |text: &str, which: i32| {
        if text.is_empty() {
            return;
        }
        let Some(btn) = lv_button_create(btn_row) else {
            return;
        };
        lv_obj_set_size(btn, btn_w, 36);
        if let Some(label) = add_label(btn, text) {
            lv_obj_center(label);
        }
        add_event_cb(btn, dialog_button_click_cb, LV_EVENT_CLICKED);
        register_button(btn.addr(), scrim_ptr, which);
    };

    add(neutral_text, BUTTON_NEUTRAL);
    add(negative_text, BUTTON_NEGATIVE);
    add(positive_text, BUTTON_POSITIVE);
}

/// Build the dialog tree. `positive_text` / `negative_text` may be empty —
/// an empty string suppresses that button. Returns the Java-side
/// `nativeHandle` of the scrim (which represents the dialog as a whole).
pub(in crate::graphics) fn create(
    title: &str,
    message: &str,
    positive_text: &str,
    negative_text: &str,
    neutral_text: &str,
) -> i32 {
    let Some((scrim, card)) = build_dialog_shell(title, message, 160) else {
        return 0;
    };
    add_button_row(
        card,
        scrim.addr(),
        positive_text,
        negative_text,
        neutral_text,
    );
    handle_table::register(scrim)
}

/// Build a list dialog: title + optional message + a button-matrix of
/// `items` (joined by `'\n'`) + the OK/cancel button row. `mode` selects
/// plain items (0), single-choice (1), or multi-choice (2); `checked_mask`
/// seeds the initial checked rows for the choice modes (bit i = row i),
/// and for plain items names the rows that are disabled.
/// Per Android, when `message` is non-empty it wins and the list is dropped.
#[allow(clippy::too_many_arguments)] // mirrors the Java nativeCreateWithList signature
pub(in crate::graphics) fn create_with_list(
    title: &str,
    message: &str,
    positive_text: &str,
    negative_text: &str,
    neutral_text: &str,
    items_joined: &str,
    mode: i32,
    checked_mask: i32,
) -> i32 {
    // Message-wins precedence (Android): fall back to a plain message dialog.
    if !message.is_empty() {
        return create(title, message, positive_text, negative_text, neutral_text);
    }

    // Split + cap the items.
    let mut item_strs: Vec<Box<[u8]>> = Vec::new();
    for seg in items_joined.split('\n') {
        if item_strs.len() == MAX_LIST_ITEMS {
            break;
        }
        let mut b = Vec::with_capacity(seg.len() + 1);
        b.extend_from_slice(seg.as_bytes());
        b.push(0);
        item_strs.push(b.into_boxed_slice());
    }
    if item_strs.is_empty() {
        return create(title, message, positive_text, negative_text, neutral_text);
    }
    // No free LIST_SLOT means we couldn't keep the matrix map alive — LVGL
    // holds the `*const *const c_char` we hand it, so dropping the storage
    // would dangle. Fall back to a plain (buttons-only) dialog rather than
    // risk a use-after-free when more than MAX_LIST_DIALOGS stack.
    let has_free_slot = STATE.list_slots.iter().any(|s| s.get().dialog_handle == 0);
    if !has_free_slot {
        return create(title, message, positive_text, negative_text, neutral_text);
    }
    let n_items = item_strs.len();

    // Taller card so the matrix has room (≈26px per row, plus chrome).
    let card_h = (90 + (n_items as i32) * 28).min(220);

    let Some((scrim, card)) = build_dialog_shell(title, "", card_h) else {
        return 0;
    };
    let scrim_ptr = scrim.addr();
    {
        // Build the matrix map: item, "\n", item, "\n", …, item, "".
        let mut map: Vec<*const c_char> = Vec::with_capacity(n_items * 2);
        for (i, it) in item_strs.iter().enumerate() {
            map.push(it.as_ptr() as *const c_char);
            if i + 1 < n_items {
                map.push(NEWLINE_SEP.as_ptr() as *const c_char);
            }
        }
        map.push(MAP_TERMINATOR.as_ptr() as *const c_char);

        if let Some(matrix) = lv_buttonmatrix_create(card) {
            lv_obj_set_size(
                matrix,
                dialog_content_width(),
                (n_items as i32 * 28).min(160),
            );
            lv_obj_set_style_bg_opa(matrix, 0, 0);
            lv_obj_set_style_pad_left(matrix, 2, 0);
            lv_obj_set_style_pad_right(matrix, 2, 0);
            lv_obj_set_style_pad_top(matrix, 2, 0);
            lv_obj_set_style_pad_bottom(matrix, 2, 0);
            // SAFETY: `matrix` was just made by `lv_buttonmatrix_create`. The map
            // and the item strings it points at are heap buffers that move into
            // `LIST_STORAGE` below (a free slot was checked for above; moving a
            // `Vec` or a `Box` keeps its buffer where it is) and are freed only
            // by `unregister_list`, which runs
            // from the scrim's delete callback or `dismiss`, as the matrix goes.
            // The separators are statics. Row ids past the last are ignored by
            // the per-button calls (`lv_buttonmatrix.c`).
            unsafe {
                lv_buttonmatrix_set_map(matrix, map.as_ptr());

                if mode == LIST_MODE_SINGLE {
                    // Radio behavior: every row checkable, only one checked at a time.
                    for i in 0..n_items as u32 {
                        lv_buttonmatrix_set_button_ctrl(matrix, i, LV_BUTTONMATRIX_CTRL_CHECKABLE);
                    }
                    lv_buttonmatrix_set_one_checked(matrix, true);
                    if checked_mask != 0 {
                        // Single-choice seeds exactly one row (lowest set bit).
                        let row = checked_mask.trailing_zeros();
                        if (row as usize) < n_items {
                            lv_buttonmatrix_set_button_ctrl(
                                matrix,
                                row,
                                LV_BUTTONMATRIX_CTRL_CHECKED,
                            );
                        }
                    }
                } else if mode == LIST_MODE_MULTI {
                    for i in 0..n_items as u32 {
                        lv_buttonmatrix_set_button_ctrl(matrix, i, LV_BUTTONMATRIX_CTRL_CHECKABLE);
                        if checked_mask & (1 << i) != 0 {
                            lv_buttonmatrix_set_button_ctrl(
                                matrix,
                                i,
                                LV_BUTTONMATRIX_CTRL_CHECKED,
                            );
                        }
                    }
                } else {
                    // Plain items: the mask is the disabled rows — an options menu's
                    // `MenuItem.setEnabled(false)`. LVGL draws such a row in its
                    // disabled state, the keypad's walk skips it, and a tap or ENTER
                    // on it sends no VALUE_CHANGED, so the dialog stays open as
                    // Android's menu does over a disabled item (QA F7).
                    for i in 0..n_items as u32 {
                        if checked_mask & (1 << i) != 0 {
                            lv_buttonmatrix_set_button_ctrl(
                                matrix,
                                i,
                                LV_BUTTONMATRIX_CTRL_DISABLED,
                            );
                        }
                    }
                }
            }

            add_event_cb(matrix, dialog_item_click_cb, LV_EVENT_VALUE_CHANGED);

            register_list(scrim_ptr, matrix.addr(), mode, item_strs, map);
        }

        add_button_row(card, scrim_ptr, positive_text, negative_text, neutral_text);
    }
    handle_table::register(scrim)
}

pub(in crate::graphics) fn show(id: i32) {
    let Some(scrim) = handle_table::get(id) else {
        return;
    };
    lv_obj_remove_flag(scrim, LV_OBJ_FLAG_HIDDEN);
    // The scrim joined the top layer after the soft-nav control did; keep
    // BACK reachable above it, as a cancelable dialog's outside stays live.
    super::super::soft_nav::raise();
    super::super::menu_button::raise();
    focus_dialog_buttons(scrim.addr());
    if let Some(oldest) = shown_push(id) {
        crate::pd_warn!(
            "AlertDialog: {} shown at once, dismissing the oldest",
            MAX_SHOWN
        );
        dismiss(oldest);
    }
}

/// Put the shown dialog's buttons in the modal keypad group and focus the
/// positive one, so ENTER activates OK on a keypad-only board (the Enviro+
/// has no touch). No-op when there's no keypad group (touch boards); the
/// buttons leave the group automatically when the scrim is deleted on
/// dismiss, and the last dismiss hands the keypad back to the Activity. See
/// project_picoenvmon_alertdialog_leak.
fn focus_dialog_buttons(scrim_ptr: usize) {
    // The dialog's own group, so NEXT/PREV stay on its buttons instead
    // of walking the rows behind the scrim; handed back on dismiss.
    let Some(group) = crate::graphics::lvgl::events::enter_modal_group() else {
        return;
    };
    // A list dialog's matrix first: it joined the Activity's group when
    // it was created (a button matrix is group-default), so move it to
    // the dialog's, where PREV/NEXT walk its rows (keypad.rs::widget_remap)
    // and SELECT picks one — and focus it, as Android focuses the list.
    let mut focus_target: Option<Obj> = None;
    for slot in STATE.list_slots.iter().map(Cell::get) {
        if slot.dialog_handle == scrim_ptr {
            // SAFETY: a list slot holds its matrix from `register_list`
            // until `unregister_list`, which the scrim's delete callback
            // runs before LVGL frees the scrim's children.
            if let Some(matrix) = unsafe { Obj::from_raw(slot.matrix as *mut lv_obj_t) } {
                lv_group_remove_obj(matrix);
                lv_group_add_obj(group, matrix);
                focus_target = Some(matrix);
            }
        }
    }
    for slot in STATE.button_map.iter().map(Cell::get) {
        if slot.dialog_handle != scrim_ptr {
            continue;
        }
        // SAFETY: a button is in the map from `register_button` until
        // `unregister_dialog`, which the scrim's delete callback runs
        // before LVGL frees the scrim's children.
        if let Some(btn) = unsafe { Obj::from_raw(slot.button_handle as *mut lv_obj_t) } {
            lv_group_add_obj(group, btn);
            if focus_target.is_none_or(|f| slot.which == BUTTON_POSITIVE && !is_matrix(f)) {
                focus_target = Some(btn);
            }
        }
    }
    if let Some(target) = focus_target {
        lv_group_focus_obj(target);
        // A keypad-focused matrix selects nothing until a key arrives,
        // and that first key would only land on row 0: start there, so
        // NEXT is row 1, as the keyboard starts on its first letter.
        if is_matrix(target) {
            // Focused from a quiet pass, so the group set FOCUSED but not
            // FOCUS_KEY, which is the state the matrix draws its selected
            // row in (as `focus_system_keyboard` does for the keyboard).
            lv_obj_add_state(target, LV_STATE_FOCUS_KEY);
            // SAFETY: `target` is in `matrix_map`, so `create_with_list`
            // made it with `lv_buttonmatrix_create`.
            unsafe { lv_buttonmatrix_set_selected_button(target, 0) };
            lv_obj_invalidate(target);
        }
    }
}

/// Whether `obj` is a list dialog's matrix.
fn is_matrix(obj: Obj) -> bool {
    STATE
        .matrix_map
        .iter()
        .any(|entry| entry.get().0 == obj.addr())
}

pub(in crate::graphics) fn dismiss(id: i32) {
    shown_remove(id); // always drop the tracking entry, even if already torn down
    if !has_shown_dialog() {
        crate::graphics::lvgl::events::leave_modal_group();
    }
    // A dialog the keypad's BACK already tore down (`dismiss_topmost_dialog`)
    // is a released handle: nothing is left to delete, and asking `lookup`
    // would be a use-after-delete to the sanitizer (QA 2026-09-13).
    if !handle_table::is_live(id) {
        return;
    }
    let Some(scrim) = handle_table::get(id) else {
        return;
    };
    let scrim_ptr = scrim.addr();
    unregister_dialog(scrim_ptr);
    unregister_list(scrim_ptr); // drops the owned matrix map/strings, if any

    // Deleting the scrim takes its children (card, buttons, labels) with
    // it via LVGL's owns-children semantics — no need to walk the tree.
    // SAFETY: `scrim` is not used after; the dialog's bookkeeping is gone,
    // its Java object is the one dismissing it, and the handle goes stale
    // through its delete hook.
    unsafe { lv_obj_delete(scrim) };
}

/// Store the owned button-matrix storage for a list dialog. The map array
/// and item strings must outlive the matrix; this keeps them alive until
/// `unregister_list` runs on dismiss.
fn register_list(
    scrim_ptr: usize,
    matrix: usize,
    mode: i32,
    items: Vec<Box<[u8]>>,
    map: Vec<*const c_char>,
) {
    if let Some(i) = STATE
        .list_slots
        .iter()
        .position(|s| s.get().dialog_handle == 0)
    {
        STATE.list_slots[i].set(ListSlot {
            dialog_handle: scrim_ptr,
            matrix,
            mode,
        });
        LIST_STORAGE[i].set(Some(ListStorage { items, _map: map }));
    }
    if let Some(entry) = STATE.matrix_map.iter().find(|e| e.get().0 == 0) {
        entry.set((matrix, scrim_ptr));
    }
}

/// Drop a dialog's list storage on dismiss. Clearing the `Vec`s frees the
/// owned map array and item strings; the matrix object itself is freed by
/// the scrim deletion.
fn unregister_list(scrim_ptr: usize) {
    for (i, slot) in STATE.list_slots.iter().enumerate() {
        let s = slot.get();
        if s.dialog_handle == scrim_ptr {
            for entry in &STATE.matrix_map {
                if entry.get().0 == s.matrix {
                    entry.set((0, 0));
                }
            }
            slot.set(EMPTY_LIST_SLOT);
            free_list_storage(i);
        }
    }
}

/// Synthetically click row `position` of a list dialog — the headless-test
/// counterpart of a real matrix tap. For choice modes it applies the same
/// checkable toggle the LVGL widget would, then enqueues the item record so
/// the full Java dispatch (`fireItemClick`) runs on the next tick.
pub(in crate::graphics) fn perform_item_click(id: i32, position: i32) {
    let Some(scrim) = handle_table::get(id) else {
        return;
    };
    if position < 0 {
        return;
    }
    let scrim_ptr = scrim.addr();
    for (slot, storage) in STATE.list_slots.iter().zip(&*LIST_STORAGE) {
        let slot = slot.get();
        if slot.dialog_handle != scrim_ptr {
            continue;
        }
        let held = storage.take();
        let n_items = held.as_ref().map_or(0, |l| l.items.len());
        // What comes back is the `None` that `take` left: nothing to drop.
        core::mem::forget(storage.replace(held));
        if position as usize >= n_items {
            return;
        }
        // SAFETY: a list slot holds its matrix, made by
        // `lv_buttonmatrix_create`, from `register_list` until
        // `unregister_list`, which the scrim's delete callback runs before
        // LVGL frees it; `pos` is below the item count, checked above.
        let Some(matrix) = (unsafe { Obj::from_raw(slot.matrix as *mut lv_obj_t) }) else {
            return;
        };
        // SAFETY: as above.
        unsafe {
            let pos = position as u32;
            let checked = if slot.mode == LIST_MODE_SINGLE {
                // Radio: one_checked clears the others; set this one.
                lv_buttonmatrix_set_button_ctrl(matrix, pos, LV_BUTTONMATRIX_CTRL_CHECKED);
                true
            } else if slot.mode == LIST_MODE_MULTI {
                let now =
                    lv_buttonmatrix_has_button_ctrl(matrix, pos, LV_BUTTONMATRIX_CTRL_CHECKED);
                if now {
                    lv_buttonmatrix_clear_button_ctrl(matrix, pos, LV_BUTTONMATRIX_CTRL_CHECKED);
                } else {
                    lv_buttonmatrix_set_button_ctrl(matrix, pos, LV_BUTTONMATRIX_CTRL_CHECKED);
                }
                !now
            } else {
                true
            };
            STATE.item_queue.push(ItemRecord {
                dialog_handle: scrim_ptr,
                position,
                checked,
            });
        }
        return;
    }
}

/// Drain one item-click event from a list dialog. Returns
/// `(dialog_handle, position, checked)`.
pub fn drain_item_click_queue() -> Option<(usize, i32, bool)> {
    let rec = STATE.item_queue.pop()?;
    Some((rec.dialog_handle, rec.position, rec.checked))
}

/// Register a Java `AlertDialog` object as the click-listener target for
/// the given dialog handle.
pub(in crate::graphics) fn register_button_click_listener(id: i32, obj_ref: u16) {
    let Some(scrim) = handle_table::get(id) else {
        return;
    };
    if let Upsert::Full = STATE.dialog_obj_map.upsert(scrim.addr(), obj_ref) {
        warn_full("alert-dialog");
    }
}

/// Drain one click event from the queue. Returns (dialog_handle, which).
pub fn drain_click_queue() -> Option<(usize, i32)> {
    let rec = STATE.click_queue.pop()?;
    Some((rec.dialog_handle, rec.which))
}

/// Visit the Java `AlertDialog` object ref of every live dialog so the GC keeps
/// it alive — a shown dialog whose Java object the app no longer references
/// would otherwise be swept, after which its button click can't dispatch. See
/// `events::visit_view_listener_roots`.
pub fn visit_dialog_obj_roots(visit: &mut dyn FnMut(u16)) {
    STATE.dialog_obj_map.visit(visit);
    STATE.dismiss_queue.for_each(visit);
}

/// Look up the Java `AlertDialog` object index for a dialog's raw scrim
/// pointer.
pub fn lookup_dialog_obj(handle: usize) -> Option<u16> {
    STATE.dialog_obj_map.lookup(handle)
}

pub fn reset_alert_dialog_state() {
    for slot in &STATE.button_map {
        slot.set(EMPTY_BUTTON);
    }
    STATE.dialog_obj_map.reset();
    STATE.click_queue.clear();
    STATE.item_queue.clear();
    STATE.dismiss_queue.clear();
    for (i, slot) in STATE.list_slots.iter().enumerate() {
        slot.set(EMPTY_LIST_SLOT);
        free_list_storage(i);
    }
    for entry in &STATE.matrix_map {
        entry.set((0, 0));
    }
    for s in &STATE.shown {
        s.set(0);
    }
    STATE.shown_len.set(0);
}

// ── Internals ───────────────────────────────────────────────────────────────

/// A new label under `parent` showing `text`.
fn add_label(parent: Obj, text: &str) -> Option<Obj> {
    let label = lv_label_create(parent)?;
    // SAFETY: `label` was just made by `lv_label_create`; the text is
    // NUL-terminated and LVGL copies it.
    super::text_view::with_cstr(text, |p| unsafe { lv_label_set_text(label, p.as_ptr()) });
    Some(label)
}

fn register_button(button_handle: usize, dialog_handle: usize, which: i32) {
    match STATE.button_map.iter().find(|s| s.get().button_handle == 0) {
        Some(slot) => slot.set(ButtonEntry {
            button_handle,
            dialog_handle,
            which,
        }),
        None => warn_full("alert-dialog-button"),
    }
}

fn unregister_dialog(scrim_ptr: usize) {
    for slot in &STATE.button_map {
        if slot.get().dialog_handle == scrim_ptr {
            slot.set(EMPTY_BUTTON);
        }
    }
    STATE.dialog_obj_map.remove(scrim_ptr);
}
