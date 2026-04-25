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
use core::ffi::c_char;

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

static mut TOAST_SLOTS: [ToastSlot; MAX_TOASTS] = [EMPTY_SLOT; MAX_TOASTS];

/// Monotonic millisecond counter accumulated from `LvglGfx::tick(ms)`. Not
/// the same clock as `system_clock::elapsed_realtime_nanos` — using the
/// tick-driven clock avoids depending on hardware time in sim builds and
/// stays drift-free with the LVGL animation loop.
static mut ELAPSED_MS: u64 = 0;

// ── LVGL ops ────────────────────────────────────────────────────────────────

/// Create a hidden toast container with `text`. Returns the Java-side
/// `nativeHandle`. The toast is parked at the bottom-center of the screen.
pub(in crate::system::picodroid::graphics) fn create(text: &str, duration: i32) -> i32 {
    let scr = lifecycle::screen_ptr();
    let toast = unsafe { lv_obj_create(scr) };

    unsafe {
        // Hidden until show() — avoids a one-frame flash before the caller
        // calls show() in the typical `Toast.makeText(...).show()` pattern.
        lv_obj_add_flag(toast, LV_OBJ_FLAG_HIDDEN);

        // Non-clickable: touches pass through to widgets underneath.
        lv_obj_remove_flag(toast, LV_OBJ_FLAG_CLICKABLE);

        // Dark, semi-transparent background to read against any content.
        lv_obj_set_style_bg_color(toast, lv_color_hex(0x303030), 0);
        lv_obj_set_style_bg_opa(toast, 220, 0);

        lv_obj_set_size(toast, 200, 40);
        // Position at lower-center for a 240×240 panel; the constants are
        // a reasonable default. Apps that need a different anchor can call
        // setPosition via View ops in a future enhancement.
        lv_obj_set_pos(toast, 20, 180);

        let label = lv_label_create(toast);
        let mut buf = [0u8; 128];
        let len = text.len().min(127);
        buf[..len].copy_from_slice(&text.as_bytes()[..len]);
        buf[len] = 0;
        lv_label_set_text(label, buf.as_ptr() as *const c_char);
        lv_obj_set_style_text_color(label, lv_color_hex(0xFFFFFF), 0);
        lv_obj_center(label);
    }

    register_pending(toast as usize, duration_to_ms(duration));
    handle_table::register(toast)
}

/// Reveal the toast and schedule its auto-dismiss.
pub(in crate::system::picodroid::graphics) fn show(id: i32) {
    let toast = handle_table::lookup(id);
    if toast.is_null() {
        return;
    }
    unsafe { lv_obj_remove_flag(toast, LV_OBJ_FLAG_HIDDEN) };
    arm(toast as usize);
}

/// Hide and delete the toast immediately.
pub(in crate::system::picodroid::graphics) fn cancel(id: i32) {
    let toast = handle_table::lookup(id);
    if toast.is_null() {
        return;
    }
    unregister(toast as usize);
    unsafe { lv_obj_delete(toast) };
}

/// Called from `LvglGfx::tick(ms)` each frame. Advances the internal clock
/// and deletes any toasts whose deadline has passed.
pub fn tick(ms: u32) {
    unsafe {
        ELAPSED_MS = ELAPSED_MS.saturating_add(ms as u64);
        let now = ELAPSED_MS;
        for slot in &mut TOAST_SLOTS[..] {
            if !slot.armed || slot.handle == 0 {
                continue;
            }
            if now >= slot.expire_at_ms {
                let toast = slot.handle as *mut lv_obj_t;
                *slot = EMPTY_SLOT;
                if !toast.is_null() {
                    lv_obj_delete(toast);
                }
            }
        }
    }
}

pub fn reset_toast_state() {
    unsafe {
        for slot in &mut TOAST_SLOTS[..] {
            *slot = EMPTY_SLOT;
        }
        ELAPSED_MS = 0;
    }
}

// ── Internals ───────────────────────────────────────────────────────────────

fn duration_to_ms(duration: i32) -> u32 {
    match duration {
        1 => LENGTH_LONG_MS,
        _ => LENGTH_SHORT_MS,
    }
}

fn register_pending(toast_ptr: usize, duration_ms: u32) {
    unsafe {
        for slot in &mut TOAST_SLOTS[..] {
            if slot.handle == 0 {
                *slot = ToastSlot {
                    handle: toast_ptr,
                    duration_ms,
                    expire_at_ms: 0,
                    armed: false,
                };
                return;
            }
        }
    }
}

fn arm(toast_ptr: usize) {
    unsafe {
        let now = ELAPSED_MS;
        for slot in &mut TOAST_SLOTS[..] {
            if slot.handle == toast_ptr {
                slot.expire_at_ms = now + slot.duration_ms as u64;
                slot.armed = true;
                return;
            }
        }
    }
}

fn unregister(toast_ptr: usize) {
    unsafe {
        for slot in &mut TOAST_SLOTS[..] {
            if slot.handle == toast_ptr {
                *slot = EMPTY_SLOT;
                return;
            }
        }
    }
}
