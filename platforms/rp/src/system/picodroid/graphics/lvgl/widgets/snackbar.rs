// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Snackbar` — a Toast with an optional clickable action
//! lozenge.
//!
//! Composition (z-order, bottom→top):
//! - `bar` — root `lv_obj_t` parented to the active screen, bottom-anchored,
//!   non-clickable (taps pass through to widgets behind it).
//! - `label` — flex-grow text, left-aligned.
//! - `action_btn` (optional, only after `set_action`) — `lv_button` with a
//!   centered child label, clickable.
//!
//! Auto-dismiss runs off the same per-frame [`tick`] heartbeat as
//! `widgets::toast` (`LvglGfx::tick`). `LENGTH_INDEFINITE` skips arming —
//! such snackbars only dismiss on explicit `dismiss()` or action tap.
//!
//! Action clicks feed a ring buffer keyed by the bar's root handle; the
//! framework loop drains it and calls `Snackbar.fireActionClick()` on the
//! matching Java object.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

const LENGTH_SHORT_MS: u32 = 2000;
const LENGTH_LONG_MS: u32 = 3500;

/// Mirror of the Java `Snackbar.LENGTH_*` constants. Anything not listed
/// here is treated as `LENGTH_SHORT`.
const DURATION_LONG: i32 = 1;
const DURATION_INDEFINITE: i32 = -1;

const MAX_SNACKBARS: usize = 4;

#[derive(Copy, Clone)]
struct SnackbarSlot {
    /// Raw `lv_obj_t*` of the bar root.
    handle: usize,
    duration_ms: u32,
    /// Absolute deadline in [`ELAPSED_MS`] units; only meaningful when
    /// `armed`. Zero before `show()` or for indefinite snackbars.
    expire_at_ms: u64,
    armed: bool,
    /// `LENGTH_INDEFINITE` snackbars never auto-dismiss. We still occupy a
    /// slot for parallel-bookkeeping so cancel() can reach them.
    indefinite: bool,
}

const EMPTY_SLOT: SnackbarSlot = SnackbarSlot {
    handle: 0,
    duration_ms: 0,
    expire_at_ms: 0,
    armed: false,
    indefinite: false,
};

static mut SLOTS: [SnackbarSlot; MAX_SNACKBARS] = [EMPTY_SLOT; MAX_SNACKBARS];

/// Monotonic millisecond counter accumulated from `LvglGfx::tick(ms)`. Same
/// model as `widgets::toast` — drift-free with the LVGL animation loop and
/// independent of hardware time so the sim builds match.
static mut ELAPSED_MS: u64 = 0;

// ── Action-click event queue (ring buffer) ──────────────────────────────────
//
// Mirror of `widgets::alert_dialog::dialog_button_click_cb` — the ring stores
// the *snackbar* root handle so `lookup_snackbar_obj` can find the Java
// `Snackbar` from a single click record.

const CLICK_QUEUE_SIZE: usize = 8;
static mut CLICK_QUEUE: [usize; CLICK_QUEUE_SIZE] = [0; CLICK_QUEUE_SIZE];
static mut CLICK_QUEUE_HEAD: usize = 0;
static mut CLICK_QUEUE_TAIL: usize = 0;

// ── Action button → snackbar mapping ────────────────────────────────────────

const MAX_ACTIONS: usize = MAX_SNACKBARS;

#[derive(Copy, Clone)]
struct ActionEntry {
    /// Raw `lv_obj_t*` of the action button.
    button_handle: usize,
    /// Raw `lv_obj_t*` of the parent snackbar bar — used to resolve the
    /// Java object on click.
    bar_handle: usize,
}

const EMPTY_ACTION: ActionEntry = ActionEntry {
    button_handle: 0,
    bar_handle: 0,
};

static mut ACTION_MAP: [ActionEntry; MAX_ACTIONS] = [EMPTY_ACTION; MAX_ACTIONS];

// ── Snackbar → Java object mapping (action-click dispatch target) ───────────

static mut SNACKBAR_OBJ_MAP: [(usize, u16); MAX_SNACKBARS] = [(0, 0); MAX_SNACKBARS];

// ── LVGL trampoline ─────────────────────────────────────────────────────────

unsafe extern "C" fn action_click_cb(e: *mut lv_event_t) {
    let btn = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let mut bar_handle: usize = 0;
        for entry in &ACTION_MAP[..] {
            if entry.button_handle == btn {
                bar_handle = entry.bar_handle;
                break;
            }
        }
        if bar_handle == 0 {
            return;
        }
        let head = CLICK_QUEUE_HEAD;
        let next = (head + 1) % CLICK_QUEUE_SIZE;
        if next != CLICK_QUEUE_TAIL {
            CLICK_QUEUE[head] = bar_handle;
            CLICK_QUEUE_HEAD = next;
        }
    }
}

// ── LVGL ops ────────────────────────────────────────────────────────────────

/// Build a hidden snackbar bar with `text`. The bar parks at the bottom-
/// center of the screen and stays hidden until [`show`] is called. Returns
/// the Java-side `nativeHandle`.
pub(in crate::system::picodroid::graphics) fn create(text: &str, duration: i32) -> i32 {
    let scr = lifecycle::screen_ptr();
    let bar = unsafe { lv_obj_create(scr) };

    unsafe {
        // Hidden until show() — avoids a one-frame flash before the typical
        // `Snackbar.make(...).show()` sequence completes.
        lv_obj_add_flag(bar, LV_OBJ_FLAG_HIDDEN);
        // Non-clickable on the bar itself; only the action lozenge accepts
        // taps. Touches outside the action pass through to widgets below.
        lv_obj_remove_flag(bar, LV_OBJ_FLAG_CLICKABLE);

        // Dark, semi-transparent background — same palette as Toast so the
        // two read consistently when an app uses both.
        lv_obj_set_style_bg_color(bar, lv_color_hex(0x303030), 0);
        lv_obj_set_style_bg_opa(bar, 230, 0);
        lv_obj_set_style_pad_left(bar, 12, 0);
        lv_obj_set_style_pad_right(bar, 6, 0);
        lv_obj_set_style_pad_top(bar, 6, 0);
        lv_obj_set_style_pad_bottom(bar, 6, 0);

        lv_obj_set_size(bar, 220, 44);
        // Bottom-center on a 240×240 panel, leaving a small gutter.
        lv_obj_set_pos(bar, 10, 188);

        // Horizontal flex: label on the left, action button on the right.
        lv_obj_set_flex_flow(bar, LV_FLEX_FLOW_ROW);
        lv_obj_set_flex_align(
            bar,
            LV_FLEX_ALIGN_START,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );

        let label = lv_label_create(bar);
        let mut buf = [0u8; 128];
        let len = text.len().min(127);
        buf[..len].copy_from_slice(&text.as_bytes()[..len]);
        buf[len] = 0;
        lv_label_set_text(label, buf.as_ptr() as *const c_char);
        lv_obj_set_style_text_color(label, lv_color_hex(0xFFFFFF), 0);
    }

    let indefinite = duration == DURATION_INDEFINITE;
    register_pending(bar as usize, duration_to_ms(duration), indefinite);
    handle_table::register(bar)
}

/// Attach (or replace) the action lozenge with `label`. Idempotent: a second
/// call replaces the prior label and re-registers the click trampoline so
/// the listener target stays current.
pub(in crate::system::picodroid::graphics) fn set_action(id: i32, label_text: &str) {
    let bar = handle_table::lookup(id);
    if bar.is_null() {
        return;
    }
    let bar_ptr = bar as usize;

    // Drop any prior action button for this bar before adding a new one.
    // This keeps `set_action` safely idempotent even though the Java side
    // currently calls it at most once.
    remove_action_for(bar_ptr);

    unsafe {
        let btn = lv_button_create(bar);
        lv_obj_set_size(btn, 70, 32);
        let lbl = lv_label_create(btn);
        let mut buf = [0u8; 32];
        let len = label_text.len().min(31);
        buf[..len].copy_from_slice(&label_text.as_bytes()[..len]);
        buf[len] = 0;
        lv_label_set_text(lbl, buf.as_ptr() as *const c_char);
        lv_obj_center(lbl);
        lv_obj_add_event_cb(
            btn,
            Some(action_click_cb),
            LV_EVENT_CLICKED,
            core::ptr::null_mut(),
        );
        register_action(btn as usize, bar_ptr);
    }
}

/// Reveal the snackbar and arm its auto-dismiss timer (no-op for
/// LENGTH_INDEFINITE).
pub(in crate::system::picodroid::graphics) fn show(id: i32) {
    let bar = handle_table::lookup(id);
    if bar.is_null() {
        return;
    }
    unsafe { lv_obj_remove_flag(bar, LV_OBJ_FLAG_HIDDEN) };
    arm(bar as usize);
}

/// Hide and delete the snackbar immediately. Detaches all bookkeeping
/// (slot, action map, obj map) before deletion so the LVGL tree teardown
/// can't dangle a click record into a freed handle.
pub(in crate::system::picodroid::graphics) fn dismiss(id: i32) {
    let bar = handle_table::lookup(id);
    if bar.is_null() {
        return;
    }
    let bar_ptr = bar as usize;
    unregister(bar_ptr);
    unsafe { lv_obj_delete(bar) };
}

/// Record a Java `Snackbar` ObjectRef as the action-click target for `id`.
pub(in crate::system::picodroid::graphics) fn register_action_click_listener(
    id: i32,
    obj_ref: u16,
) {
    let bar_ptr = handle_table::lookup(id) as usize;
    if bar_ptr == 0 {
        return;
    }
    unsafe {
        for entry in &mut SNACKBAR_OBJ_MAP[..] {
            if entry.0 == bar_ptr {
                entry.1 = obj_ref;
                return;
            }
            if entry.0 == 0 {
                *entry = (bar_ptr, obj_ref);
                return;
            }
        }
    }
}

/// Per-frame tick — advances the elapsed clock and dismisses any snackbars
/// past their deadline. Wired from `LvglGfx::tick` next to `toast::tick`.
pub fn tick(ms: u32) {
    unsafe {
        ELAPSED_MS = ELAPSED_MS.saturating_add(ms as u64);
        let now = ELAPSED_MS;
        for slot in &mut SLOTS[..] {
            if !slot.armed || slot.handle == 0 || slot.indefinite {
                continue;
            }
            if now >= slot.expire_at_ms {
                let bar = slot.handle as *mut lv_obj_t;
                let bar_ptr = slot.handle;
                *slot = EMPTY_SLOT;
                // Detach action map + obj map for this bar before deletion
                // — same rationale as `dismiss`.
                detach_action_for(bar_ptr);
                detach_obj_for(bar_ptr);
                if !bar.is_null() {
                    lv_obj_delete(bar);
                }
            }
        }
    }
}

/// Drain one queued action click (raw bar `lv_obj_t*` value).
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_click_queue() -> Option<usize> {
    unsafe {
        if CLICK_QUEUE_TAIL == CLICK_QUEUE_HEAD {
            return None;
        }
        let h = CLICK_QUEUE[CLICK_QUEUE_TAIL];
        CLICK_QUEUE_TAIL = (CLICK_QUEUE_TAIL + 1) % CLICK_QUEUE_SIZE;
        Some(h)
    }
}

/// Look up the Java `Snackbar` ObjectRef for a bar's raw pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_snackbar_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &SNACKBAR_OBJ_MAP[..] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_snackbar_state() {
    unsafe {
        for slot in &mut SLOTS[..] {
            *slot = EMPTY_SLOT;
        }
        for entry in &mut ACTION_MAP[..] {
            *entry = EMPTY_ACTION;
        }
        for entry in &mut SNACKBAR_OBJ_MAP[..] {
            *entry = (0, 0);
        }
        for slot in &mut CLICK_QUEUE[..] {
            *slot = 0;
        }
        CLICK_QUEUE_HEAD = 0;
        CLICK_QUEUE_TAIL = 0;
        ELAPSED_MS = 0;
    }
}

// ── Internals ───────────────────────────────────────────────────────────────

fn duration_to_ms(duration: i32) -> u32 {
    match duration {
        DURATION_LONG => LENGTH_LONG_MS,
        DURATION_INDEFINITE => 0,
        // DURATION_SHORT and any unknown value.
        _ => LENGTH_SHORT_MS,
    }
}

fn register_pending(bar_ptr: usize, duration_ms: u32, indefinite: bool) {
    unsafe {
        for slot in &mut SLOTS[..] {
            if slot.handle == 0 {
                *slot = SnackbarSlot {
                    handle: bar_ptr,
                    duration_ms,
                    expire_at_ms: 0,
                    armed: false,
                    indefinite,
                };
                return;
            }
        }
    }
    // Slot table full: caller's snackbar still renders but won't auto-
    // dismiss until something frees a slot. Matches Toast's overflow
    // semantics; not worth a panic on an MCU.
    let _ = (bar_ptr, duration_ms, indefinite);
}

fn arm(bar_ptr: usize) {
    unsafe {
        let now = ELAPSED_MS;
        for slot in &mut SLOTS[..] {
            if slot.handle == bar_ptr {
                if slot.indefinite {
                    slot.armed = true;
                } else {
                    slot.expire_at_ms = now + slot.duration_ms as u64;
                    slot.armed = true;
                }
                return;
            }
        }
    }
}

fn register_action(button_handle: usize, bar_handle: usize) {
    unsafe {
        for slot in &mut ACTION_MAP[..] {
            if slot.button_handle == 0 {
                *slot = ActionEntry {
                    button_handle,
                    bar_handle,
                };
                return;
            }
        }
    }
}

fn remove_action_for(bar_ptr: usize) {
    unsafe {
        for slot in &mut ACTION_MAP[..] {
            if slot.bar_handle == bar_ptr {
                let btn = slot.button_handle as *mut lv_obj_t;
                *slot = EMPTY_ACTION;
                if !btn.is_null() {
                    lv_obj_delete(btn);
                }
            }
        }
    }
}

fn detach_action_for(bar_ptr: usize) {
    unsafe {
        for slot in &mut ACTION_MAP[..] {
            if slot.bar_handle == bar_ptr {
                *slot = EMPTY_ACTION;
            }
        }
    }
}

fn detach_obj_for(bar_ptr: usize) {
    unsafe {
        for entry in &mut SNACKBAR_OBJ_MAP[..] {
            if entry.0 == bar_ptr {
                *entry = (0, 0);
            }
        }
    }
}

fn unregister(bar_ptr: usize) {
    unsafe {
        for slot in &mut SLOTS[..] {
            if slot.handle == bar_ptr {
                *slot = EMPTY_SLOT;
            }
        }
    }
    detach_action_for(bar_ptr);
    detach_obj_for(bar_ptr);
}
