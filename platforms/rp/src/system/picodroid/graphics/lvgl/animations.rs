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

// Interpolator codes — must mirror the constants on
// `picodroid.view.animation.*`. The native tick can't upcall into a custom
// Java Interpolator per frame, so only these four are honored; anything else
// falls back to linear (the Java side logs a warning).
const INTERP_LINEAR: i32 = 0;
const INTERP_ACCELERATE: i32 = 1;
const INTERP_DECELERATE: i32 = 2;
const INTERP_ACCEL_DECEL: i32 = 3;

/// Fixed-point scale for the eased progress fraction (1.0 == `EASE_SCALE`).
/// Keeps the easing math in bounded integers — no FPU on RP2040.
const EASE_SCALE: i64 = 4096;

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
    interpolator: i32,
    active: bool,
}

const EMPTY_ANIM: AnimSlot = AnimSlot {
    handle: 0,
    property: 0,
    from: 0,
    to: 0,
    duration_ms: 0,
    elapsed_ms: 0,
    interpolator: INTERP_LINEAR,
    active: false,
};

static mut ANIM_SLOTS: [AnimSlot; MAX_ANIMATIONS] = [EMPTY_ANIM; MAX_ANIMATIONS];

// ── End-action storage (Android's withEndAction) ────────────────────────────
//
// A Runnable per animating handle, fired once when that handle's last active
// animation completes. Keyed by handle so a multi-property chain (which runs
// as several same-duration slots) fires the action exactly once. Cleared
// without firing on cancel — Android skips withEndAction on cancel.

const MAX_END_ACTIONS: usize = 8;
static mut END_ACTIONS: [(i32, u16); MAX_END_ACTIONS] = [(0, 0); MAX_END_ACTIONS];

// Completion queue: obj_refs of end-action Runnables whose animations just
// finished, drained by the lifecycle loop and run through the Executors
// bytecode bridge (lambda proxies only resolve there).
const COMPLETION_QUEUE_SIZE: usize = 8;
static mut COMPLETION_QUEUE: [u16; COMPLETION_QUEUE_SIZE] = [0; COMPLETION_QUEUE_SIZE];
static mut COMPLETION_HEAD: usize = 0;
static mut COMPLETION_TAIL: usize = 0;

/// Begin a new animation. Replaces any active animation for the same
/// `(handle, property)` pair so re-issuing `view.animate().alpha(...)`
/// doesn't pile up old slots.
pub fn start(handle: i32, property: i32, from: i32, to: i32, duration_ms: u32, interpolator: i32) {
    if duration_ms == 0 {
        // Zero-duration is a snap, not an animation. Apply once and skip
        // the slot — saves a frame of useless interpolation work.
        apply(handle, property, to);
        return;
    }
    let new_slot = AnimSlot {
        handle,
        property,
        from,
        to,
        duration_ms,
        elapsed_ms: 0,
        interpolator,
        active: true,
    };
    unsafe {
        // Replace existing same-property anim if one is running.
        for slot in &mut ANIM_SLOTS[..] {
            if slot.active && slot.handle == handle && slot.property == property {
                *slot = new_slot;
                return;
            }
        }
        // Else find empty slot.
        for slot in &mut ANIM_SLOTS[..] {
            if !slot.active {
                *slot = new_slot;
                return;
            }
        }
        // Slot table full — silently drop. Apps that hit this are likely
        // animating dozens of widgets concurrently which isn't viable on
        // this platform anyway.
    }
}

/// Register a Runnable to fire once `handle`'s animations complete (Android's
/// `withEndAction`). Replaces any existing action for the handle.
pub fn set_end_action(handle: i32, obj_ref: u16) {
    unsafe {
        for entry in &mut END_ACTIONS[..] {
            if entry.0 == handle {
                entry.1 = obj_ref;
                return;
            }
        }
        for entry in &mut END_ACTIONS[..] {
            if entry.0 == 0 {
                *entry = (handle, obj_ref);
                return;
            }
        }
    }
}

/// Drop `handle`'s end action without firing it (cancel path).
fn clear_end_action(handle: i32) {
    unsafe {
        for entry in &mut END_ACTIONS[..] {
            if entry.0 == handle {
                *entry = (0, 0);
            }
        }
    }
}

/// Fire `handle`'s end action (enqueue its Runnable) if one is registered and
/// no other slot for `handle` is still animating. Called when a slot retires.
unsafe fn maybe_fire_end_action(handle: i32) {
    for slot in &ANIM_SLOTS[..] {
        if slot.active && slot.handle == handle {
            return; // another property of this view is still animating
        }
    }
    for entry in &mut END_ACTIONS[..] {
        if entry.0 == handle && entry.1 != 0 {
            let obj_ref = entry.1;
            *entry = (0, 0);
            let next = (COMPLETION_HEAD + 1) % COMPLETION_QUEUE_SIZE;
            if next != COMPLETION_TAIL {
                COMPLETION_QUEUE[COMPLETION_HEAD] = obj_ref;
                COMPLETION_HEAD = next;
            }
            return;
        }
    }
}

/// Drain one completed end-action Runnable obj_ref, if any.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_completed_end_action() -> Option<u16> {
    unsafe {
        if COMPLETION_TAIL == COMPLETION_HEAD {
            return None;
        }
        let r = COMPLETION_QUEUE[COMPLETION_TAIL];
        COMPLETION_TAIL = (COMPLETION_TAIL + 1) % COMPLETION_QUEUE_SIZE;
        Some(r)
    }
}

/// GC roots for pending end-action Runnables — a withEndAction lambda kept
/// alive only by this native map would otherwise be swept before it runs
/// (exactly the historical click/dialog-map bug class).
pub fn visit_end_action_roots(visit: &mut dyn FnMut(u16)) {
    unsafe {
        for &(_, r) in &END_ACTIONS[..] {
            if r != 0 {
                visit(r);
            }
        }
        let mut i = COMPLETION_TAIL;
        while i != COMPLETION_HEAD {
            let r = COMPLETION_QUEUE[i];
            if r != 0 {
                visit(r);
            }
            i = (i + 1) % COMPLETION_QUEUE_SIZE;
        }
    }
}

/// Apply the slot's interpolator to a linear progress fraction `p` (0..=`EASE_SCALE`),
/// returning the eased fraction on the same scale.
fn ease(interpolator: i32, p: i64) -> i64 {
    let s = EASE_SCALE;
    match interpolator {
        INTERP_ACCELERATE => p * p / s, // t²
        INTERP_DECELERATE => {
            let q = s - p;
            s - q * q / s // 1 - (1-t)²
        }
        INTERP_ACCEL_DECEL => {
            // t²(3 - 2t) on the 0..s scale: p²(3s - 2p) / s².
            p * p * (3 * s - 2 * p) / (s * s)
        }
        _ => p, // linear (and unknown → linear)
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
    // Android skips withEndAction on cancel — drop it without firing.
    clear_end_action(handle);
}

/// Cancel every animation whose target view is `root` or a descendant of it.
/// MUST be called from the view-delete path *before* the LVGL objects are
/// freed, while each slot's handle still resolves to a live object.
///
/// This is the safety net that the per-frame [`apply`] null-check cannot be on
/// 32-bit (RP2040/RP2350): there a `nativeHandle` *is* the raw `lv_obj_t*`
/// (see `handle_table`), so a deleted view's handle never becomes null — it
/// dangles. Ticking such a slot dereferences freed LVGL memory, and the freed
/// object's display reads back NULL, tripping LVGL's `LV_ASSERT_NULL(disp)` →
/// `while(1)` hang (observed: backing out of the picoenvmon Live screen while a
/// `flashOnBreach` tile alpha animation was still running). 64-bit/sim never
/// hit this because its handle table invalidates deleted slots to null.
pub fn cancel_subtree(root: *mut lv_obj_t) {
    if root.is_null() {
        return;
    }
    unsafe {
        for slot in &mut ANIM_SLOTS[..] {
            if !slot.active {
                continue;
            }
            // Walk up from the animated object; if we reach `root` it is in the
            // subtree being deleted. Resolved now, before lv_obj_delete frees it.
            let mut cur = handle_table::lookup(slot.handle);
            while !cur.is_null() {
                if cur == root {
                    let handle = slot.handle;
                    *slot = EMPTY_ANIM;
                    // Drop the end action without firing — the view is gone.
                    clear_end_action(handle);
                    break;
                }
                cur = lv_obj_get_parent(cur);
            }
        }
    }
}

/// Called once per frame from `LvglGfx::tick(ms)` — advances each active
/// slot, applies the interpolated value, and clears slots whose deadline
/// has passed.
pub fn tick(ms: u32) {
    // Handles whose slot retired this tick — checked for end-action firing
    // after the main loop so we don't read ANIM_SLOTS while iterating it `mut`.
    let mut retired: [i32; MAX_ANIMATIONS] = [0; MAX_ANIMATIONS];
    let mut retired_len = 0usize;
    unsafe {
        for slot in &mut ANIM_SLOTS[..] {
            if !slot.active {
                continue;
            }
            slot.elapsed_ms = slot.elapsed_ms.saturating_add(ms);
            let value = if slot.elapsed_ms >= slot.duration_ms {
                slot.to
            } else {
                // Normalize progress to the 0..EASE_SCALE fixed-point fraction,
                // apply the interpolator, then interpolate. All i64 to avoid
                // overflow; the bounded fraction keeps the easing products small.
                let t_den = slot.duration_ms.max(1) as i64;
                let p = (slot.elapsed_ms as i64 * EASE_SCALE / t_den).min(EASE_SCALE);
                let eased = ease(slot.interpolator, p);
                let delta = (slot.to - slot.from) as i64;
                slot.from + (delta * eased / EASE_SCALE) as i32
            };
            apply(slot.handle, slot.property, value);
            if slot.elapsed_ms >= slot.duration_ms {
                slot.active = false;
                retired[retired_len] = slot.handle;
                retired_len += 1;
            }
        }
        // Fire end actions for any handle whose last slot just retired.
        for &h in &retired[..retired_len] {
            maybe_fire_end_action(h);
        }
    }
}

pub fn reset_animation_state() {
    unsafe {
        for slot in &mut ANIM_SLOTS[..] {
            *slot = EMPTY_ANIM;
        }
        for entry in &mut END_ACTIONS[..] {
            *entry = (0, 0);
        }
        COMPLETION_HEAD = 0;
        COMPLETION_TAIL = 0;
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
