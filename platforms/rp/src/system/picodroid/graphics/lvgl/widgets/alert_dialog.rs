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
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

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

static mut CLICK_QUEUE: [ClickRecord; CLICK_QUEUE_SIZE] = [EMPTY_CLICK; CLICK_QUEUE_SIZE];
static mut CLICK_QUEUE_HEAD: usize = 0;
static mut CLICK_QUEUE_TAIL: usize = 0;

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

static mut ITEM_QUEUE: [ItemRecord; CLICK_QUEUE_SIZE] = [EMPTY_ITEM; CLICK_QUEUE_SIZE];
static mut ITEM_QUEUE_HEAD: usize = 0;
static mut ITEM_QUEUE_TAIL: usize = 0;

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

struct ListSlot {
    dialog_handle: usize,
    /// The matrix object — its mode controls how clicks are interpreted.
    matrix: usize,
    mode: i32,
    /// Owned NUL-terminated item strings; addresses must stay stable while
    /// the matrix lives, so each is its own boxed slice.
    items: Vec<Box<[u8]>>,
    /// The map array LVGL points at: item ptr, "\n", item ptr, …, "". Boxed
    /// so its address is stable across the Vec's own moves.
    map: Vec<*const c_char>,
}

const EMPTY_LIST_SLOT: ListSlot = ListSlot {
    dialog_handle: 0,
    matrix: 0,
    mode: LIST_MODE_ITEMS,
    items: Vec::new(),
    map: Vec::new(),
};

static mut LIST_SLOTS: [ListSlot; MAX_LIST_DIALOGS] = [
    EMPTY_LIST_SLOT,
    EMPTY_LIST_SLOT,
    EMPTY_LIST_SLOT,
    EMPTY_LIST_SLOT,
];

/// Map of `(matrix ptr → dialog scrim ptr)` so the VALUE_CHANGED trampoline
/// can find the owning dialog (and its list slot) from the fired matrix.
static mut MATRIX_MAP: [(usize, usize); MAX_LIST_DIALOGS] = [(0, 0); MAX_LIST_DIALOGS];

// ── Button → dialog mapping (each LVGL button knows its parent dialog) ──────
//
// Two entries per dialog max (positive + negative). Stored as
// (button_ptr, dialog_ptr, which).

const MAX_BUTTONS: usize = 16;

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

static mut BUTTON_MAP: [ButtonEntry; MAX_BUTTONS] = [EMPTY_BUTTON; MAX_BUTTONS];

// ── Dialog → Java object mapping (for click dispatch) ───────────────────────

const MAX_DIALOGS: usize = 8;
static mut DIALOG_OBJ_MAP: [(usize, u16); MAX_DIALOGS] = [(0, 0); MAX_DIALOGS];

// ── Shown-dialog stack ──────────────────────────────────────────────────────
//
// Java `nativeHandle` ids of dialogs currently on screen, newest on top. Lets
// the framework dismiss the topmost dialog on BACK (Android's cancelable
// default) and tear down any dialog an Activity leaves up when it finishes —
// the scrim is parented to the screen, so it would otherwise outlive the
// Activity and leak onto the one beneath as an input-absorbing modal.

const MAX_SHOWN: usize = 4;
static mut SHOWN: [i32; MAX_SHOWN] = [0; MAX_SHOWN];
static mut SHOWN_LEN: usize = 0;

fn shown_push(id: i32) {
    unsafe {
        shown_remove(id); // de-dup so re-show moves it to the top
        if SHOWN_LEN < MAX_SHOWN {
            SHOWN[SHOWN_LEN] = id;
            SHOWN_LEN += 1;
        }
    }
}

fn shown_remove(id: i32) {
    unsafe {
        // `&mut SHOWN[..]` (indexed slice) rather than letting a method autoref
        // the whole static — the latter trips `static_mut_refs`; this matches
        // the `&mut BUTTON_MAP[..]` idiom used elsewhere in this file.
        let len = SHOWN_LEN;
        let s = &mut SHOWN[..];
        if let Some(pos) = s[..len].iter().position(|&x| x == id) {
            s.copy_within(pos + 1..len, pos); // left-shift over the hole
            s[len - 1] = 0;
            SHOWN_LEN = len - 1;
        }
    }
}

/// True if any dialog is currently on screen.
pub fn has_shown_dialog() -> bool {
    unsafe { SHOWN_LEN > 0 }
}

/// Dismiss the most-recently-shown dialog (BACK / cancel). Returns `true` if
/// one was dismissed, `false` if none were showing.
pub fn dismiss_topmost_dialog() -> bool {
    let id = unsafe {
        if SHOWN_LEN == 0 {
            return false;
        }
        SHOWN[SHOWN_LEN - 1]
    };
    dismiss(id); // also pops it from SHOWN
    true
}

// ── LVGL trampoline ─────────────────────────────────────────────────────────

unsafe extern "C" fn dialog_button_click_cb(e: *mut lv_event_t) {
    let btn = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        // Look up which dialog this button belongs to.
        let mut found: Option<(usize, i32)> = None;
        for entry in &BUTTON_MAP[..] {
            if entry.button_handle == btn {
                found = Some((entry.dialog_handle, entry.which));
                break;
            }
        }
        let (dialog_handle, which) = match found {
            Some(x) => x,
            None => return,
        };

        let head = CLICK_QUEUE_HEAD;
        let next = (head + 1) % CLICK_QUEUE_SIZE;
        if next != CLICK_QUEUE_TAIL {
            CLICK_QUEUE[head] = ClickRecord {
                dialog_handle,
                which,
            };
            CLICK_QUEUE_HEAD = next;
        }
    }
}

/// VALUE_CHANGED trampoline for a list dialog's button-matrix. Enqueues the
/// selected row index plus (for choice lists) its post-click checked state.
unsafe extern "C" fn dialog_item_click_cb(e: *mut lv_event_t) {
    let matrix = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let mut dialog_handle = 0usize;
        for &(m, d) in &MATRIX_MAP[..] {
            if m == matrix {
                dialog_handle = d;
                break;
            }
        }
        if dialog_handle == 0 {
            return;
        }
        let sel = lv_buttonmatrix_get_selected_button(matrix as *const lv_obj_t);
        if sel == LV_BUTTONMATRIX_BUTTON_NONE {
            return;
        }
        // For checkable (choice) lists, report the row's post-click checked
        // state; for plain item lists every click is a select (true).
        let checked = lv_buttonmatrix_has_button_ctrl(
            matrix as *mut lv_obj_t,
            sel,
            LV_BUTTONMATRIX_CTRL_CHECKED,
        );

        let head = ITEM_QUEUE_HEAD;
        let next = (head + 1) % CLICK_QUEUE_SIZE;
        if next != ITEM_QUEUE_TAIL {
            ITEM_QUEUE[head] = ItemRecord {
                dialog_handle,
                position: sel as i32,
                checked,
            };
            ITEM_QUEUE_HEAD = next;
        }
    }
}

// ── LVGL ops (called from widgets/alert_dialog.rs Java shim) ────────────────

/// Build the modal scrim + card and add the title/message labels. Returns
/// `(scrim, card)`; the caller appends any list matrix and then the button
/// row (flex-column order = creation order). `card` height defaults to 160
/// but is overridden by list dialogs to fit the matrix.
unsafe fn build_dialog_shell(
    title: &str,
    message: &str,
    card_h: i32,
) -> (*mut lv_obj_t, *mut lv_obj_t) {
    let scr = lifecycle::screen_ptr();
    let scrim = lv_obj_create(scr);

    // Modal scrim: fullscreen, dim, click-absorbing.
    lv_obj_add_flag(scrim, LV_OBJ_FLAG_HIDDEN);
    lv_obj_add_flag(scrim, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_set_pos(scrim, 0, 0);
    lv_obj_set_size(scrim, 240, 240);
    lv_obj_set_style_bg_color(scrim, lv_color_hex(0x000000), 0);
    lv_obj_set_style_bg_opa(scrim, 160, 0);
    lv_obj_set_style_pad_left(scrim, 0, 0);
    lv_obj_set_style_pad_right(scrim, 0, 0);
    lv_obj_set_style_pad_top(scrim, 0, 0);
    lv_obj_set_style_pad_bottom(scrim, 0, 0);

    // Card: vertical flex with title, message, and a button row.
    let card = lv_obj_create(scrim);
    lv_obj_set_size(card, 200, card_h);
    // Center the card inside the scrim's 240×240. Vertical offset keeps the
    // card centered as it grows for list content.
    let card_y = ((240 - card_h) / 2).max(8);
    lv_obj_set_pos(card, 20, card_y);
    lv_obj_set_style_bg_color(card, lv_color_hex(0xFFFFFF), 0);
    lv_obj_set_style_bg_opa(card, LV_OPA_COVER, 0);
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
        let title_label = lv_label_create(card);
        set_label_text(title_label, title);
        lv_obj_set_style_text_color(title_label, lv_color_hex(0x000000), 0);
    }

    if !message.is_empty() {
        let msg_label = lv_label_create(card);
        set_label_text(msg_label, message);
        lv_obj_set_style_text_color(msg_label, lv_color_hex(0x303030), 0);
    }

    (scrim, card)
}

/// Append the title/cancel button row to `card`. Empty strings suppress the
/// corresponding button. Neutral is leftmost, then negative, then positive
/// (Android placement).
unsafe fn add_button_row(
    card: *mut lv_obj_t,
    scrim_ptr: usize,
    positive_text: &str,
    negative_text: &str,
    neutral_text: &str,
) {
    let btn_row = lv_obj_create(card);
    lv_obj_set_size(btn_row, 176, 50);
    lv_obj_set_style_bg_opa(btn_row, 0, 0);
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

    // Three buttons share the 176px row only at a narrower width; with one
    // or two, keep the roomier 80px Android-ish buttons.
    let n_buttons = (!positive_text.is_empty()) as i32
        + (!negative_text.is_empty()) as i32
        + (!neutral_text.is_empty()) as i32;
    let btn_w = if n_buttons >= 3 { 54 } else { 80 };

    let add = |text: &str, which: i32| {
        if text.is_empty() {
            return;
        }
        let btn = lv_button_create(btn_row);
        lv_obj_set_size(btn, btn_w, 36);
        let label = lv_label_create(btn);
        set_label_text(label, text);
        lv_obj_center(label);
        lv_obj_add_event_cb(
            btn,
            Some(dialog_button_click_cb),
            LV_EVENT_CLICKED,
            core::ptr::null_mut(),
        );
        register_button(btn as usize, scrim_ptr, which);
    };

    add(neutral_text, BUTTON_NEUTRAL);
    add(negative_text, BUTTON_NEGATIVE);
    add(positive_text, BUTTON_POSITIVE);
}

/// Build the dialog tree. `positive_text` / `negative_text` may be empty —
/// an empty string suppresses that button. Returns the Java-side
/// `nativeHandle` of the scrim (which represents the dialog as a whole).
pub(in crate::system::picodroid::graphics) fn create(
    title: &str,
    message: &str,
    positive_text: &str,
    negative_text: &str,
    neutral_text: &str,
) -> i32 {
    let scrim = unsafe {
        let (scrim, card) = build_dialog_shell(title, message, 160);
        add_button_row(
            card,
            scrim as usize,
            positive_text,
            negative_text,
            neutral_text,
        );
        scrim
    };
    handle_table::register(scrim)
}

/// Build a list dialog: title + optional message + a button-matrix of
/// `items` (joined by `'\n'`) + the OK/cancel button row. `mode` selects
/// plain items (0), single-choice (1), or multi-choice (2); `checked_mask`
/// seeds the initial checked rows for the choice modes (bit i = row i).
/// Per Android, when `message` is non-empty it wins and the list is dropped.
#[allow(clippy::too_many_arguments)] // mirrors the Java nativeCreateWithList signature
pub(in crate::system::picodroid::graphics) fn create_with_list(
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
    let has_free_slot = unsafe { LIST_SLOTS[..].iter().any(|s| s.dialog_handle == 0) };
    if !has_free_slot {
        return create(title, message, positive_text, negative_text, neutral_text);
    }
    let n_items = item_strs.len();

    // Taller card so the matrix has room (≈26px per row, plus chrome).
    let card_h = (90 + (n_items as i32) * 28).min(220);

    let scrim = unsafe {
        let (scrim, card) = build_dialog_shell(title, "", card_h);
        let scrim_ptr = scrim as usize;

        // Build the matrix map: item, "\n", item, "\n", …, item, "".
        let mut map: Vec<*const c_char> = Vec::with_capacity(n_items * 2);
        for (i, it) in item_strs.iter().enumerate() {
            map.push(it.as_ptr() as *const c_char);
            if i + 1 < n_items {
                map.push(NEWLINE_SEP.as_ptr() as *const c_char);
            }
        }
        map.push(MAP_TERMINATOR.as_ptr() as *const c_char);

        let matrix = lv_buttonmatrix_create(card);
        lv_obj_set_size(matrix, 176, (n_items as i32 * 28).min(160));
        lv_obj_set_style_bg_opa(matrix, 0, 0);
        lv_obj_set_style_pad_left(matrix, 2, 0);
        lv_obj_set_style_pad_right(matrix, 2, 0);
        lv_obj_set_style_pad_top(matrix, 2, 0);
        lv_obj_set_style_pad_bottom(matrix, 2, 0);
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
                    lv_buttonmatrix_set_button_ctrl(matrix, row, LV_BUTTONMATRIX_CTRL_CHECKED);
                }
            }
        } else if mode == LIST_MODE_MULTI {
            for i in 0..n_items as u32 {
                lv_buttonmatrix_set_button_ctrl(matrix, i, LV_BUTTONMATRIX_CTRL_CHECKABLE);
                if checked_mask & (1 << i) != 0 {
                    lv_buttonmatrix_set_button_ctrl(matrix, i, LV_BUTTONMATRIX_CTRL_CHECKED);
                }
            }
        }

        lv_obj_add_event_cb(
            matrix,
            Some(dialog_item_click_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );

        register_list(scrim_ptr, matrix as usize, mode, item_strs, map);

        add_button_row(card, scrim_ptr, positive_text, negative_text, neutral_text);
        scrim
    };
    handle_table::register(scrim)
}

pub(in crate::system::picodroid::graphics) fn show(id: i32) {
    let scrim = handle_table::lookup(id);
    if scrim.is_null() {
        return;
    }
    unsafe { lv_obj_remove_flag(scrim, LV_OBJ_FLAG_HIDDEN) };
    focus_dialog_buttons(scrim as usize);
    shown_push(id);
}

/// Add the shown dialog's buttons to the active keypad focus group and focus
/// the positive one, so ENTER activates OK on a keypad-only board (the Enviro+
/// has no touch). No-op when there's no default group (touch boards return
/// null); the buttons leave the group automatically when the scrim is deleted
/// on dismiss. See project_picoenvmon_alertdialog_leak.
fn focus_dialog_buttons(scrim_ptr: usize) {
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return;
        }
        let mut focus_target: *mut lv_obj_t = core::ptr::null_mut();
        for slot in &BUTTON_MAP[..] {
            if slot.button_handle != 0 && slot.dialog_handle == scrim_ptr {
                let btn = slot.button_handle as *mut lv_obj_t;
                lv_group_add_obj(group, btn);
                if focus_target.is_null() || slot.which == BUTTON_POSITIVE {
                    focus_target = btn;
                }
            }
        }
        if !focus_target.is_null() {
            lv_group_focus_obj(focus_target);
        }
    }
}

pub(in crate::system::picodroid::graphics) fn dismiss(id: i32) {
    shown_remove(id); // always drop the tracking entry, even if already torn down
    let scrim = handle_table::lookup(id);
    if scrim.is_null() {
        return;
    }
    let scrim_ptr = scrim as usize;
    unregister_dialog(scrim_ptr);
    unregister_list(scrim_ptr); // drops the owned matrix map/strings, if any
                                // Deleting the scrim takes its children (card, buttons, labels) with
                                // it via LVGL's owns-children semantics — no need to walk the tree.
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
    unsafe {
        for slot in &mut LIST_SLOTS[..] {
            if slot.dialog_handle == 0 {
                slot.dialog_handle = scrim_ptr;
                slot.matrix = matrix;
                slot.mode = mode;
                slot.items = items;
                slot.map = map;
                break;
            }
        }
        for entry in &mut MATRIX_MAP[..] {
            if entry.0 == 0 {
                *entry = (matrix, scrim_ptr);
                break;
            }
        }
    }
}

/// Drop a dialog's list storage on dismiss. Clearing the `Vec`s frees the
/// owned map array and item strings; the matrix object itself is freed by
/// the scrim deletion.
fn unregister_list(scrim_ptr: usize) {
    unsafe {
        for slot in &mut LIST_SLOTS[..] {
            if slot.dialog_handle == scrim_ptr {
                for entry in &mut MATRIX_MAP[..] {
                    if entry.0 == slot.matrix {
                        *entry = (0, 0);
                    }
                }
                slot.dialog_handle = 0;
                slot.matrix = 0;
                slot.mode = LIST_MODE_ITEMS;
                slot.items = Vec::new();
                slot.map = Vec::new();
            }
        }
    }
}

/// Synthetically click row `position` of a list dialog — the headless-test
/// counterpart of a real matrix tap. For choice modes it applies the same
/// checkable toggle the LVGL widget would, then enqueues the item record so
/// the full Java dispatch (`fireItemClick`) runs on the next tick.
pub(in crate::system::picodroid::graphics) fn perform_item_click(id: i32, position: i32) {
    let scrim = handle_table::lookup(id);
    if scrim.is_null() || position < 0 {
        return;
    }
    let scrim_ptr = scrim as usize;
    unsafe {
        for slot in &mut LIST_SLOTS[..] {
            if slot.dialog_handle != scrim_ptr {
                continue;
            }
            if position as usize >= slot.items.len() {
                return;
            }
            let matrix = slot.matrix as *mut lv_obj_t;
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
            let head = ITEM_QUEUE_HEAD;
            let next = (head + 1) % CLICK_QUEUE_SIZE;
            if next != ITEM_QUEUE_TAIL {
                ITEM_QUEUE[head] = ItemRecord {
                    dialog_handle: scrim_ptr,
                    position,
                    checked,
                };
                ITEM_QUEUE_HEAD = next;
            }
            return;
        }
    }
}

/// Drain one item-click event from a list dialog. Returns
/// `(dialog_handle, position, checked)`.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_item_click_queue() -> Option<(usize, i32, bool)> {
    unsafe {
        if ITEM_QUEUE_TAIL == ITEM_QUEUE_HEAD {
            return None;
        }
        let rec = ITEM_QUEUE[ITEM_QUEUE_TAIL];
        ITEM_QUEUE_TAIL = (ITEM_QUEUE_TAIL + 1) % CLICK_QUEUE_SIZE;
        Some((rec.dialog_handle, rec.position, rec.checked))
    }
}

/// Register a Java `AlertDialog` object as the click-listener target for
/// the given dialog handle.
pub(in crate::system::picodroid::graphics) fn register_button_click_listener(
    id: i32,
    obj_ref: u16,
) {
    let scrim_ptr = handle_table::lookup(id) as usize;
    if scrim_ptr == 0 {
        return;
    }
    unsafe {
        for entry in &mut DIALOG_OBJ_MAP[..] {
            if entry.0 == scrim_ptr {
                entry.1 = obj_ref;
                return;
            }
            if entry.0 == 0 {
                *entry = (scrim_ptr, obj_ref);
                return;
            }
        }
    }
}

/// Drain one click event from the queue. Returns (dialog_handle, which).
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_click_queue() -> Option<(usize, i32)> {
    unsafe {
        if CLICK_QUEUE_TAIL == CLICK_QUEUE_HEAD {
            return None;
        }
        let rec = CLICK_QUEUE[CLICK_QUEUE_TAIL];
        CLICK_QUEUE_TAIL = (CLICK_QUEUE_TAIL + 1) % CLICK_QUEUE_SIZE;
        Some((rec.dialog_handle, rec.which))
    }
}

/// Visit the Java `AlertDialog` object ref of every live dialog so the GC keeps
/// it alive — a shown dialog whose Java object the app no longer references
/// would otherwise be swept, after which its button click can't dispatch. See
/// `events::visit_view_listener_roots`.
pub fn visit_dialog_obj_roots(visit: &mut dyn FnMut(u16)) {
    unsafe {
        for &(_, r) in &DIALOG_OBJ_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
    }
}

/// Look up the Java `AlertDialog` object index for a dialog's raw scrim
/// pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_dialog_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &DIALOG_OBJ_MAP[..] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_alert_dialog_state() {
    unsafe {
        for slot in &mut BUTTON_MAP[..] {
            *slot = EMPTY_BUTTON;
        }
        for entry in &mut DIALOG_OBJ_MAP[..] {
            *entry = (0, 0);
        }
        for slot in &mut CLICK_QUEUE[..] {
            *slot = EMPTY_CLICK;
        }
        CLICK_QUEUE_HEAD = 0;
        CLICK_QUEUE_TAIL = 0;
        for slot in &mut ITEM_QUEUE[..] {
            *slot = EMPTY_ITEM;
        }
        ITEM_QUEUE_HEAD = 0;
        ITEM_QUEUE_TAIL = 0;
        for slot in &mut LIST_SLOTS[..] {
            slot.dialog_handle = 0;
            slot.matrix = 0;
            slot.mode = LIST_MODE_ITEMS;
            slot.items = Vec::new();
            slot.map = Vec::new();
        }
        for entry in &mut MATRIX_MAP[..] {
            *entry = (0, 0);
        }
        for s in &mut SHOWN[..] {
            *s = 0;
        }
        SHOWN_LEN = 0;
    }
}

// ── Internals ───────────────────────────────────────────────────────────────

unsafe fn set_label_text(label: *mut lv_obj_t, text: &str) {
    let mut buf = [0u8; 192];
    let len = text.len().min(191);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_label_set_text(label, buf.as_ptr() as *const c_char) };
}

fn register_button(button_handle: usize, dialog_handle: usize, which: i32) {
    unsafe {
        for slot in &mut BUTTON_MAP[..] {
            if slot.button_handle == 0 {
                *slot = ButtonEntry {
                    button_handle,
                    dialog_handle,
                    which,
                };
                return;
            }
        }
    }
}

fn unregister_dialog(scrim_ptr: usize) {
    unsafe {
        for slot in &mut BUTTON_MAP[..] {
            if slot.dialog_handle == scrim_ptr {
                *slot = EMPTY_BUTTON;
            }
        }
        for entry in &mut DIALOG_OBJ_MAP[..] {
            if entry.0 == scrim_ptr {
                *entry = (0, 0);
            }
        }
    }
}
