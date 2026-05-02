// SPDX-License-Identifier: GPL-3.0-only
//! View property animations.
//!
//! A small static slot table polled from [`LvglGfx::tick(ms)`] every frame.
//! Each slot animates one property (alpha / x / y) of one Java
//! `nativeHandle` from a `from` value to a `to` value over `duration_ms`.
//!
//! We do *not* use LVGL's `lv_anim_*` engine. The reasons:
//!
//! - The slot-table-polled-from-tick pattern is already proven by
//!   [`super::widgets::toast::tick`]; staying consistent keeps the FFI
//!   surface curated (per the project's convention) and avoids growing
//!   `lvgl_ffi.rs` with `lv_anim_t` struct layout assumptions.
//! - The animation engine handles its own timing — see the
//!   `feedback_no_handler_postdelayed.md` memory: this is the home for
//!   "delayed work", not a user-facing scheduler.
//!
//! Interpolation is linear in v1. Easing curves (ease-in-out, etc.) and
//! per-animation completion listeners are planned follow-ups.

use crate::lvgl_ffi::*;

use super::handle_table;

// ── Property codes — must mirror the constants on
//    `picodroid.view.ViewPropertyAnimator`.

const PROPERTY_ALPHA: i32 = 0;
const PROPERTY_X: i32 = 1;
const PROPERTY_Y: i32 = 2;

const MAX_ANIMATIONS: usize = 16;

#[derive(Copy, Clone)]
struct AnimSlot {
    /// Java `nativeHandle` of the View being animated.
    handle: i32,
    property: i32,
    from: i32,
    to: i32,
    duration_ms: u32,
    elapsed_ms: u32,
    active: bool,
}

const EMPTY_ANIM: AnimSlot = AnimSlot {
    handle: 0,
    property: 0,
    from: 0,
    to: 0,
    duration_ms: 0,
    elapsed_ms: 0,
    active: false,
};

static mut ANIM_SLOTS: [AnimSlot; MAX_ANIMATIONS] = [EMPTY_ANIM; MAX_ANIMATIONS];

/// Begin a new animation. Replaces any active animation for the same
/// `(handle, property)` pair so re-issuing `view.animate().alpha(...)`
/// doesn't pile up old slots.
pub fn start(handle: i32, property: i32, from: i32, to: i32, duration_ms: u32) {
    if duration_ms == 0 {
        // Zero-duration is a snap, not an animation. Apply once and skip
        // the slot — saves a frame of useless interpolation work.
        apply(handle, property, to);
        return;
    }
    unsafe {
        // Replace existing same-property anim if one is running.
        for slot in &mut ANIM_SLOTS[..] {
            if slot.active && slot.handle == handle && slot.property == property {
                *slot = AnimSlot {
                    handle,
                    property,
                    from,
                    to,
                    duration_ms,
                    elapsed_ms: 0,
                    active: true,
                };
                return;
            }
        }
        // Else find empty slot.
        for slot in &mut ANIM_SLOTS[..] {
            if !slot.active {
                *slot = AnimSlot {
                    handle,
                    property,
                    from,
                    to,
                    duration_ms,
                    elapsed_ms: 0,
                    active: true,
                };
                return;
            }
        }
        // Slot table full — silently drop. Apps that hit this are likely
        // animating dozens of widgets concurrently which isn't viable on
        // this platform anyway.
    }
}

/// Cancel every animation targeting `handle`. Called by Java
/// `ViewPropertyAnimator.cancel()`. The view's current property values
/// remain at whatever the last frame left them — Android does the same.
pub fn cancel(handle: i32) {
    unsafe {
        for slot in &mut ANIM_SLOTS[..] {
            if slot.active && slot.handle == handle {
                *slot = EMPTY_ANIM;
            }
        }
    }
}

/// Called once per frame from `LvglGfx::tick(ms)` — advances each active
/// slot, applies the interpolated value, and clears slots whose deadline
/// has passed.
pub fn tick(ms: u32) {
    unsafe {
        for slot in &mut ANIM_SLOTS[..] {
            if !slot.active {
                continue;
            }
            slot.elapsed_ms = slot.elapsed_ms.saturating_add(ms);
            let value = if slot.elapsed_ms >= slot.duration_ms {
                slot.to
            } else {
                // Linear interpolation, kept in i64 to avoid overflow on
                // multiplications with elapsed/duration up to ~2^31 each.
                let t_num = slot.elapsed_ms as i64;
                let t_den = slot.duration_ms.max(1) as i64;
                let delta = (slot.to - slot.from) as i64;
                slot.from + (delta * t_num / t_den) as i32
            };
            apply(slot.handle, slot.property, value);
            if slot.elapsed_ms >= slot.duration_ms {
                slot.active = false;
            }
        }
    }
}

pub fn reset_animation_state() {
    unsafe {
        for slot in &mut ANIM_SLOTS[..] {
            *slot = EMPTY_ANIM;
        }
    }
}

// ── Property setters ────────────────────────────────────────────────────────

fn apply(handle: i32, property: i32, value: i32) {
    let obj = handle_table::lookup(handle);
    if obj.is_null() {
        // The view was deleted out from under the animation — silently
        // drop the rest of the slot via the elapsed-check in `tick` (the
        // null obj means no LVGL FFI call). We don't proactively clear
        // the slot here because handle_table::lookup is non-allocating
        // and the slot will retire on its own deadline.
        return;
    }
    unsafe {
        match property {
            PROPERTY_ALPHA => {
                let alpha = value.clamp(0, 255) as u8;
                lv_obj_set_style_opa(obj, alpha, 0);
            }
            PROPERTY_X => lv_obj_set_x(obj, value),
            PROPERTY_Y => lv_obj_set_y(obj, value),
            _ => {} // unknown property — silently ignore
        }
    }
}
