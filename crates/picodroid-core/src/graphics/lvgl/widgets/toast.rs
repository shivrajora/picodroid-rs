// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `Toast` — a non-modal floating label that auto-dismisses
//! after a fixed duration.
//!
//! The toast object is a plain `lv_obj_t` parented directly to the active
//! screen, so it renders above the content view (LVGL draws siblings in
//! creation order). It is *not* clickable — touches pass through to the
//! widgets underneath, matching Android's Toast semantics.
//!
//! Auto-dismiss is driven by [`tick`], scanned from `LvglGfx::tick` every
//! frame. We deliberately do *not* expose an LVGL `lv_timer_create` FFI
//! surface — the per-frame scan is simpler and matches the existing
//! ring-buffer-style state in this crate.

use crate::lvgl_ffi::*;
use crate::util::local::Core0;
use core::cell::Cell;

use super::super::handle_table;
use super::super::lifecycle;

const LENGTH_SHORT_MS: u32 = 2000;
const LENGTH_LONG_MS: u32 = 3500;

/// Maximum number of toasts that can be alive at once. New `show()` calls
/// past this limit silently drop their auto-dismiss registration; the
/// toast will still render but stay until the app explicitly dismisses it.
const MAX_TOASTS: usize = 4;

#[derive(Copy, Clone)]
struct ToastSlot {
    handle: usize,
    duration_ms: u32,
    /// Absolute deadline in [`ELAPSED_MS`] units. Only meaningful when
    /// `armed`; before show() this is 0.
    expire_at_ms: u64,
    armed: bool,
}

const EMPTY_SLOT: ToastSlot = ToastSlot {
    handle: 0,
    duration_ms: 0,
    expire_at_ms: 0,
    armed: false,
};

struct State {
    slots: [Cell<ToastSlot>; MAX_TOASTS],
    /// Monotonic millisecond counter accumulated from `LvglGfx::tick(ms)`.
    /// Not the same clock as `system_clock::elapsed_realtime_nanos` — using
    /// the tick-driven clock avoids depending on hardware time in sim builds
    /// and stays drift-free with the LVGL animation loop.
    elapsed_ms: Cell<u64>,
}

// SAFETY: written by natives and the frame tick, both on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        slots: [const { Cell::new(EMPTY_SLOT) }; MAX_TOASTS],
        elapsed_ms: Cell::new(0),
    })
};

// ── LVGL ops ────────────────────────────────────────────────────────────────

/// The toast's box for a window `w` x `h` pixels: `(width, height, x, y)`. A
/// lozenge 40 px tall, inset 20 px from each side up to 280 px wide, resting
/// 20 px above the bottom edge. The same 200x40 at (20,180) a 240x240 panel
/// always had; a wider or taller window moves it, a 320x480 panel's toast is
/// no longer at a third of the height.
fn toast_box(w: i32, h: i32) -> (i32, i32, i32, i32) {
    let width = (w - 40).clamp(200, 280).min(w);
    let height = 40;
    (width, height, (w - width) / 2, (h - height - 20).max(0))
}

/// Create a hidden toast container with `text`. Returns the Java-side
/// `nativeHandle`. The toast is parked at the bottom-center of the window,
/// on the overlay layer so it survives a `setContentView` and stays put
/// while a screen pans.
pub(in crate::graphics) fn create(text: &str, duration: i32) -> i32 {
    let layer = lifecycle::overlay_layer();
    // SAFETY: the top layer is a live LVGL object for the display's lifetime.
    let toast = unsafe { lv_obj_create(layer) };
    let (win_w, win_h) = lifecycle::window_size();
    let (width, height, x, y) = toast_box(win_w, win_h);

    unsafe {
        // Hidden until show() — avoids a one-frame flash before the caller
        // calls show() in the typical `Toast.makeText(...).show()` pattern.
        lv_obj_add_flag(toast, LV_OBJ_FLAG_HIDDEN);

        // Non-clickable: touches pass through to widgets underneath.
        lv_obj_remove_flag(toast, LV_OBJ_FLAG_CLICKABLE);

        // Dark, semi-transparent background to read against any content.
        lv_obj_set_style_bg_color(toast, lv_color_hex(0x303030), 0);
        lv_obj_set_style_bg_opa(toast, 220, 0);

        lv_obj_set_size(toast, width, height);
        lv_obj_set_pos(toast, x, y);

        let label = lv_label_create(toast);
        super::text_view::with_cstr(text, |p| lv_label_set_text(label, p));
        lv_obj_set_style_text_color(label, lv_color_hex(0xFFFFFF), 0);
        lv_obj_center(label);
    }

    register_pending(toast as usize, duration_to_ms(duration));
    handle_table::register(toast)
}

/// Reveal the toast and schedule its auto-dismiss.
pub(in crate::graphics) fn show(id: i32) {
    let toast = handle_table::lookup(id);
    if toast.is_null() {
        return;
    }
    unsafe { lv_obj_remove_flag(toast, LV_OBJ_FLAG_HIDDEN) };
    arm(toast as usize);
}

/// Hide and delete the toast immediately.
pub(in crate::graphics) fn cancel(id: i32) {
    let toast = handle_table::lookup(id);
    if toast.is_null() {
        return;
    }
    unregister(toast as usize);
    unsafe { lv_obj_delete(toast) };
}

/// Update the auto-dismiss duration of a not-yet-shown toast. An armed
/// (already showing) toast keeps its original deadline — the new duration
/// applies if `show()` arms it again, matching the Java-side javadoc.
pub(in crate::graphics) fn set_duration(id: i32, duration: i32) {
    let toast = handle_table::lookup(id);
    if toast.is_null() {
        return;
    }
    let ms = duration_to_ms(duration);
    if let Some(slot) = STATE
        .slots
        .iter()
        .find(|s| s.get().handle == toast as usize)
    {
        slot.set(ToastSlot {
            duration_ms: ms,
            ..slot.get()
        });
    }
}

/// Called from `LvglGfx::tick(ms)` each frame. Advances the internal clock
/// and deletes any toasts whose deadline has passed.
pub fn tick(ms: u32) {
    let elapsed = &STATE.elapsed_ms;
    elapsed.set(elapsed.get().saturating_add(ms as u64));
    let now = elapsed.get();
    for slot in &STATE.slots {
        let s = slot.get();
        if !s.armed || s.handle == 0 {
            continue;
        }
        if now >= s.expire_at_ms {
            slot.set(EMPTY_SLOT);
            super::super::view_ops::delete_obj(s.handle as *mut lv_obj_t);
        }
    }
}

pub fn reset_toast_state() {
    for slot in &STATE.slots {
        slot.set(EMPTY_SLOT);
    }
    STATE.elapsed_ms.set(0);
}

// ── Internals ───────────────────────────────────────────────────────────────

fn duration_to_ms(duration: i32) -> u32 {
    match duration {
        1 => LENGTH_LONG_MS,
        _ => LENGTH_SHORT_MS,
    }
}

fn register_pending(toast_ptr: usize, duration_ms: u32) {
    if let Some(slot) = STATE.slots.iter().find(|s| s.get().handle == 0) {
        slot.set(ToastSlot {
            handle: toast_ptr,
            duration_ms,
            expire_at_ms: 0,
            armed: false,
        });
    }
}

fn arm(toast_ptr: usize) {
    let now = STATE.elapsed_ms.get();
    if let Some(slot) = STATE.slots.iter().find(|s| s.get().handle == toast_ptr) {
        let s = slot.get();
        slot.set(ToastSlot {
            expire_at_ms: now + s.duration_ms as u64,
            armed: true,
            ..s
        });
    }
}

fn unregister(toast_ptr: usize) {
    if let Some(slot) = STATE.slots.iter().find(|s| s.get().handle == toast_ptr) {
        slot.set(EMPTY_SLOT);
    }
}
