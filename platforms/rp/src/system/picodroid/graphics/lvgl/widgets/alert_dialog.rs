// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `AlertDialog` — a modal dialog with title/message and up to
//! two buttons (positive / negative).
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
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

const BUTTON_POSITIVE: i32 = 0;
const BUTTON_NEGATIVE: i32 = 1;

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

// ── LVGL ops (called from widgets/alert_dialog.rs Java shim) ────────────────

/// Build the dialog tree. `positive_text` / `negative_text` may be empty —
/// an empty string suppresses that button. Returns the Java-side
/// `nativeHandle` of the scrim (which represents the dialog as a whole).
pub(in crate::system::picodroid::graphics) fn create(
    title: &str,
    message: &str,
    positive_text: &str,
    negative_text: &str,
) -> i32 {
    let scr = lifecycle::screen_ptr();
    let scrim = unsafe { lv_obj_create(scr) };

    unsafe {
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
        lv_obj_set_size(card, 200, 160);
        // Center the card inside the scrim. The scrim is 240×240 fullscreen
        // and has zero padding (set above), so an offset of (20, 40) lands
        // the 200×160 card roughly centered with a slight top weight.
        lv_obj_set_pos(card, 20, 40);
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

        // Button row: horizontal flex pinned to the bottom of the card.
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

        let scrim_ptr = scrim as usize;

        if !negative_text.is_empty() {
            // Order matters: in flex-row layout, negative goes left of
            // positive (matches Material guidelines and Android convention).
            let neg_btn = lv_button_create(btn_row);
            lv_obj_set_size(neg_btn, 80, 36);
            let neg_label = lv_label_create(neg_btn);
            set_label_text(neg_label, negative_text);
            lv_obj_center(neg_label);
            lv_obj_add_event_cb(
                neg_btn,
                Some(dialog_button_click_cb),
                LV_EVENT_CLICKED,
                core::ptr::null_mut(),
            );
            register_button(neg_btn as usize, scrim_ptr, BUTTON_NEGATIVE);
        }

        if !positive_text.is_empty() {
            let pos_btn = lv_button_create(btn_row);
            lv_obj_set_size(pos_btn, 80, 36);
            let pos_label = lv_label_create(pos_btn);
            set_label_text(pos_label, positive_text);
            lv_obj_center(pos_label);
            lv_obj_add_event_cb(
                pos_btn,
                Some(dialog_button_click_cb),
                LV_EVENT_CLICKED,
                core::ptr::null_mut(),
            );
            register_button(pos_btn as usize, scrim_ptr, BUTTON_POSITIVE);
        }
    }

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
    // Deleting the scrim takes its children (card, buttons, labels) with
    // it via LVGL's owns-children semantics — no need to walk the tree.
    unsafe { lv_obj_delete(scrim) };
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
