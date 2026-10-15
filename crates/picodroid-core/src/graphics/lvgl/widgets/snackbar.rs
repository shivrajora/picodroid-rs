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
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

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

// ── Action-click event queue (ring buffer) ──────────────────────────────────
//
// Mirror of `widgets::alert_dialog::dialog_button_click_cb` — the ring stores
// the *snackbar* root handle so `lookup_snackbar_obj` can find the Java
// `Snackbar` from a single click record.

const CLICK_QUEUE_SIZE: usize = 8;

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

struct State {
    slots: [Cell<SnackbarSlot>; MAX_SNACKBARS],
    /// Monotonic millisecond counter accumulated from `LvglGfx::tick(ms)`.
    /// Same model as `widgets::toast` — drift-free with the LVGL animation
    /// loop and independent of hardware time so the sim builds match.
    elapsed_ms: Cell<u64>,
    click_queue: LocalRing<usize, CLICK_QUEUE_SIZE>,
    action_map: [Cell<ActionEntry>; MAX_ACTIONS],
    /// Snackbar → Java object mapping (the action-click dispatch target).
    obj_map: [Cell<(usize, u16)>; MAX_SNACKBARS],
}

// SAFETY: filled by natives and the frame tick, read by LVGL's click
// callback, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        slots: [const { Cell::new(EMPTY_SLOT) }; MAX_SNACKBARS],
        elapsed_ms: Cell::new(0),
        click_queue: LocalRing::new(0),
        action_map: [const { Cell::new(EMPTY_ACTION) }; MAX_ACTIONS],
        obj_map: [const { Cell::new((0, 0)) }; MAX_SNACKBARS],
    })
};

// ── LVGL trampoline ─────────────────────────────────────────────────────────

lv_event_cb! {
    fn action_click_cb(e) {
        let btn = e.target().addr();
        let bar_handle = STATE
            .action_map
            .iter()
            .map(Cell::get)
            .find(|entry| entry.button_handle == btn)
            .map_or(0, |entry| entry.bar_handle);
        if bar_handle == 0 {
            return;
        }
        STATE.click_queue.push(bar_handle);
    }
}

// ── LVGL ops ────────────────────────────────────────────────────────────────

/// The bar's box for a window `w` pixels wide: `(width, x, bottom_gap)`.
/// Full width less a 10 px gutter each side, 8 px above the bottom edge:
/// the 220 wide at x 10, bottom at 232, a 240x240 panel always had,
/// following the window on the others. The height follows the content —
/// 44 px for a one-line message beside the 32 px action — so a message
/// that wraps grows the bar upward rather than past its edge (QA F4).
fn bar_box(w: i32) -> (i32, i32, i32) {
    ((w - 20).max(0), 10, 8)
}

/// Build a hidden snackbar bar with `text`. The bar parks at the bottom-
/// center of the window, on the overlay layer (see `lifecycle::overlay_layer`),
/// and stays hidden until [`show`] is called. Returns the Java-side
/// `nativeHandle`.
pub(in crate::graphics) fn create(text: &str, duration: i32) -> i32 {
    let Some(bar) = lifecycle::overlay_layer().and_then(|o| lv_obj_create(o)) else {
        return 0;
    };
    let (win_w, _) = lifecycle::window_size();
    let (width, x, bottom_gap) = bar_box(win_w);

    {
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

        // Never a scroll container: a message wider than the room beside the
        // action wraps (below) instead of pushing the action past the edge
        // behind a scrollbar.
        lv_obj_remove_flag(bar, LV_OBJ_FLAG_SCROLLABLE);
        lv_obj_set_scrollbar_mode(bar, LV_SCROLLBAR_MODE_OFF);
        lv_obj_set_size(bar, width, LV_SIZE_CONTENT);
        lv_obj_set_align(bar, LV_ALIGN_BOTTOM_LEFT);
        lv_obj_set_pos(bar, x, -bottom_gap);

        // Horizontal flex: label on the left, action button on the right.
        lv_obj_set_flex_flow(bar, LV_FLEX_FLOW_ROW);
        lv_obj_set_flex_align(
            bar,
            LV_FLEX_ALIGN_START,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );

        if let Some(label) = lv_label_create(bar) {
            // SAFETY: `label` was just made by `lv_label_create`; the text
            // is NUL-terminated and LVGL copies it.
            super::text_view::with_cstr(text, |p| unsafe { lv_label_set_text(label, p.as_ptr()) });
            lv_obj_set_style_text_color(label, lv_color_hex(0xFFFFFF), 0);
            // The width left beside the action, wrapping onto further lines;
            // the action keeps its own width at the end of the row.
            lv_obj_set_flex_grow(label, 1);
            // SAFETY: `label` was just made by `lv_label_create`.
            unsafe { lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_WRAP) };
        }
    }

    let indefinite = duration == DURATION_INDEFINITE;
    register_pending(bar.addr(), duration_to_ms(duration), indefinite);
    handle_table::register(bar)
}

/// Attach (or replace) the action lozenge with `label`. Idempotent: a second
/// call replaces the prior label and re-registers the click trampoline so
/// the listener target stays current.
pub(in crate::graphics) fn set_action(id: i32, label_text: &str) {
    let Some(bar) = handle_table::get(id) else {
        return;
    };
    let bar_ptr = bar.addr();

    // Drop any prior action button for this bar before adding a new one.
    // This keeps `set_action` safely idempotent even though the Java side
    // currently calls it at most once.
    remove_action_for(bar_ptr);

    let Some(btn) = lv_button_create(bar) else {
        return;
    };
    lv_obj_set_size(btn, 70, 32);
    if let Some(lbl) = lv_label_create(btn) {
        // SAFETY: `lbl` was just made by `lv_label_create`; the text is
        // NUL-terminated and LVGL copies it.
        super::text_view::with_cstr(label_text, |p| unsafe {
            lv_label_set_text(lbl, p.as_ptr())
        });
        lv_obj_center(lbl);
    }
    add_event_cb(btn, action_click_cb, LV_EVENT_CLICKED);
    register_action(btn.addr(), bar_ptr);
}

/// Reveal the snackbar and arm its auto-dismiss timer (no-op for
/// LENGTH_INDEFINITE).
pub(in crate::graphics) fn show(id: i32) {
    let Some(bar) = handle_table::get(id) else {
        return;
    };
    lv_obj_remove_flag(bar, LV_OBJ_FLAG_HIDDEN);
    arm(bar.addr());
}

/// Hide and delete the snackbar immediately. Detaches all bookkeeping
/// (slot, action map, obj map) before deletion so the LVGL tree teardown
/// can't dangle a click record into a freed handle.
pub(in crate::graphics) fn dismiss(id: i32) {
    let Some(bar) = handle_table::get(id) else {
        return;
    };
    unregister(bar.addr());
    // SAFETY: `bar` is not used after, its bookkeeping is gone, and the
    // handle goes stale through its delete hook.
    unsafe { super::super::view_ops::delete_obj(bar) };
}

/// Record a Java `Snackbar` ObjectRef as the action-click target for `id`.
pub(in crate::graphics) fn register_action_click_listener(id: i32, obj_ref: u16) {
    let Some(bar) = handle_table::get(id) else {
        return;
    };
    let bar_ptr = bar.addr();
    for entry in &STATE.obj_map {
        let (bar, _) = entry.get();
        if bar == bar_ptr || bar == 0 {
            entry.set((bar_ptr, obj_ref));
            return;
        }
    }
}

/// Per-frame tick — advances the elapsed clock and dismisses any snackbars
/// past their deadline. Wired from `LvglGfx::tick` next to `toast::tick`.
pub fn tick(ms: u32) {
    let elapsed = &STATE.elapsed_ms;
    elapsed.set(elapsed.get().saturating_add(ms as u64));
    let now = elapsed.get();
    for slot in &STATE.slots {
        let s = slot.get();
        if !s.armed || s.handle == 0 || s.indefinite {
            continue;
        }
        if now >= s.expire_at_ms {
            let bar_ptr = s.handle;
            slot.set(EMPTY_SLOT);
            // Detach action map + obj map for this bar before deletion
            // — same rationale as `dismiss`.
            detach_action_for(bar_ptr);
            detach_obj_for(bar_ptr);
            // SAFETY: a slot holds a bar from `create` until `dismiss` or
            // this tick takes it out, and nothing else deletes a bar: it
            // lives on the top layer, which nothing cleans. Its bookkeeping
            // is gone, and the handle goes stale through its delete hook.
            if let Some(bar) = unsafe { Obj::from_raw(bar_ptr as *mut lv_obj_t) } {
                // SAFETY: as above; no other copy of `bar` is used after.
                unsafe { super::super::view_ops::delete_obj(bar) };
            }
        }
    }
}

/// Drain one queued action click (raw bar `lv_obj_t*` value).
pub fn drain_click_queue() -> Option<usize> {
    STATE.click_queue.pop()
}

/// Look up the Java `Snackbar` ObjectRef for a bar's raw pointer.
pub fn lookup_snackbar_obj(handle: usize) -> Option<u16> {
    STATE
        .obj_map
        .iter()
        .map(Cell::get)
        .find(|&(bar, _)| bar == handle)
        .map(|(_, obj_ref)| obj_ref)
}

pub fn reset_snackbar_state() {
    for slot in &STATE.slots {
        slot.set(EMPTY_SLOT);
    }
    for entry in &STATE.action_map {
        entry.set(EMPTY_ACTION);
    }
    for entry in &STATE.obj_map {
        entry.set((0, 0));
    }
    STATE.click_queue.clear();
    STATE.elapsed_ms.set(0);
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
    if let Some(slot) = STATE.slots.iter().find(|s| s.get().handle == 0) {
        slot.set(SnackbarSlot {
            handle: bar_ptr,
            duration_ms,
            expire_at_ms: 0,
            armed: false,
            indefinite,
        });
        return;
    }
    // Slot table full: caller's snackbar still renders but won't auto-
    // dismiss until something frees a slot. Matches Toast's overflow
    // semantics; not worth a panic on an MCU.
    let _ = (bar_ptr, duration_ms, indefinite);
}

fn arm(bar_ptr: usize) {
    let now = STATE.elapsed_ms.get();
    if let Some(slot) = STATE.slots.iter().find(|s| s.get().handle == bar_ptr) {
        let mut s = slot.get();
        if !s.indefinite {
            s.expire_at_ms = now + s.duration_ms as u64;
        }
        s.armed = true;
        slot.set(s);
    }
}

fn register_action(button_handle: usize, bar_handle: usize) {
    if let Some(slot) = STATE.action_map.iter().find(|s| s.get().button_handle == 0) {
        slot.set(ActionEntry {
            button_handle,
            bar_handle,
        });
    }
}

fn remove_action_for(bar_ptr: usize) {
    for slot in &STATE.action_map {
        let entry = slot.get();
        if entry.bar_handle == bar_ptr {
            slot.set(EMPTY_ACTION);
            // SAFETY: an action button is in the map from `set_action` until
            // its bar's bookkeeping goes (`unregister`, the tick) — before
            // the bar, and the button with it, is deleted — or this call
            // deletes it, its entry already cleared. No handle names it.
            if let Some(btn) = unsafe { Obj::from_raw(entry.button_handle as *mut lv_obj_t) } {
                // SAFETY: as above; nothing uses `btn` after.
                unsafe { lv_obj_delete(btn) };
            }
        }
    }
}

fn detach_action_for(bar_ptr: usize) {
    for slot in &STATE.action_map {
        if slot.get().bar_handle == bar_ptr {
            slot.set(EMPTY_ACTION);
        }
    }
}

fn detach_obj_for(bar_ptr: usize) {
    for entry in &STATE.obj_map {
        if entry.get().0 == bar_ptr {
            entry.set((0, 0));
        }
    }
}

fn unregister(bar_ptr: usize) {
    for slot in &STATE.slots {
        if slot.get().handle == bar_ptr {
            slot.set(EMPTY_SLOT);
        }
    }
    detach_action_for(bar_ptr);
    detach_obj_for(bar_ptr);
}
