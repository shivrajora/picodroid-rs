// SPDX-License-Identifier: GPL-3.0-only
//! View property animations.
//!
//! A small static slot table polled from [`LvglGfx::tick(ms)`] every frame.
//! Each slot animates one property (alpha / x / y / translation / rotation /
//! scale) of one Java `nativeHandle` from a `from` value to a `to` value over
//! `duration_ms`, after an optional `delay_ms`.
//!
//! The Java API is to-only (`view.animate().alpha(0f)`), as on Android. The
//! implicit `from` is read back from LVGL here: at [`start_to`] for an
//! immediate animation, or when the delay expires for a delayed one — so a
//! delayed animation takes over from wherever a running one has got to.
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
//! Units: every value in a slot is in LVGL's integer domain — opacity
//! 0..=255, pixels, rotation in 0.1°, scale with `LV_SCALE_NONE` (256) = 1.0.
//! The Android-facing floats are converted exactly once, in [`to_units`] /
//! [`from_units`], so `View.setRotation` and `animate().rotation` agree.

use crate::lvgl_ffi::*;

use super::handle_table;

// ── Property codes — must mirror the constants on
//    `picodroid.view.ViewPropertyAnimator` (also used by `View.nativeSetProperty`).

pub const PROPERTY_ALPHA: i32 = 0;
pub const PROPERTY_X: i32 = 1;
pub const PROPERTY_Y: i32 = 2;
pub const PROPERTY_TRANSLATION_X: i32 = 3;
pub const PROPERTY_TRANSLATION_Y: i32 = 4;
pub const PROPERTY_ROTATION: i32 = 5;
pub const PROPERTY_SCALE_X: i32 = 6;
pub const PROPERTY_SCALE_Y: i32 = 7;

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
    /// Remaining start delay; the slot is *pending* while this is non-zero.
    delay_ms: u32,
    interpolator: i32,
    /// `from` has not been captured yet — read it from LVGL when the delay
    /// expires, so a delayed start begins from the then-current value.
    from_pending: bool,
    active: bool,
}

const EMPTY_ANIM: AnimSlot = AnimSlot {
    handle: 0,
    property: 0,
    from: 0,
    to: 0,
    duration_ms: 0,
    elapsed_ms: 0,
    delay_ms: 0,
    interpolator: INTERP_LINEAR,
    from_pending: false,
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

// ── Units ───────────────────────────────────────────────────────────────────

/// Round half away from zero without `std` (`f32::round` needs libm).
fn round_i32(v: f32) -> i32 {
    if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    }
}

/// Android-units float → LVGL integer units for `property`.
pub fn to_units(property: i32, value: f32) -> i32 {
    match property {
        PROPERTY_ALPHA => round_i32(value.clamp(0.0, 1.0) * 255.0),
        PROPERTY_ROTATION => round_i32(value * 10.0),
        PROPERTY_SCALE_X | PROPERTY_SCALE_Y => round_i32(value.max(0.0) * (LV_SCALE_NONE as f32)),
        _ => round_i32(value),
    }
}

/// LVGL integer units → Android-units float for `property`.
pub fn from_units(property: i32, value: i32) -> f32 {
    match property {
        PROPERTY_ALPHA => value as f32 / 255.0,
        PROPERTY_ROTATION => value as f32 / 10.0,
        PROPERTY_SCALE_X | PROPERTY_SCALE_Y => value as f32 / (LV_SCALE_NONE as f32),
        _ => value as f32,
    }
}

fn is_transform(property: i32) -> bool {
    matches!(
        property,
        PROPERTY_ROTATION | PROPERTY_SCALE_X | PROPERTY_SCALE_Y
    )
}

// ── LVGL readback ───────────────────────────────────────────────────────────

/// A numeric style property of `obj`'s main part. Unset props come back as
/// LVGL's defaults: opa 255, scale `LV_SCALE_NONE`, translate / rotation 0.
unsafe fn style_num(obj: *const lv_obj_t, prop: lv_style_prop_t) -> i32 {
    lv_obj_get_style_prop(obj, LV_PART_MAIN, prop).num
}

/// The view's translation (x, y) in pixels. LVGL folds `translate_*` into
/// the laid-out coords, so the geometry reader (`view_ops::frame`) subtracts
/// this to report Android's translation-free `getLeft` / `getTop`.
pub(super) fn translation_of(obj: *const lv_obj_t) -> (i32, i32) {
    unsafe {
        (
            style_num(obj, LV_STYLE_TRANSLATE_X),
            style_num(obj, LV_STYLE_TRANSLATE_Y),
        )
    }
}

/// The property's current value in LVGL units — the implicit `from`.
unsafe fn read_current(obj: *mut lv_obj_t, property: i32) -> i32 {
    match property {
        PROPERTY_ALPHA => style_num(obj, LV_STYLE_OPA),
        PROPERTY_X => {
            // Coords are stale until a layout pass, and include translation.
            lv_obj_update_layout(obj);
            lv_obj_get_x(obj) - style_num(obj, LV_STYLE_TRANSLATE_X)
        }
        PROPERTY_Y => {
            lv_obj_update_layout(obj);
            lv_obj_get_y(obj) - style_num(obj, LV_STYLE_TRANSLATE_Y)
        }
        PROPERTY_TRANSLATION_X => style_num(obj, LV_STYLE_TRANSLATE_X),
        PROPERTY_TRANSLATION_Y => style_num(obj, LV_STYLE_TRANSLATE_Y),
        PROPERTY_ROTATION => style_num(obj, LV_STYLE_TRANSFORM_ROTATION),
        PROPERTY_SCALE_X => style_num(obj, LV_STYLE_TRANSFORM_SCALE_X),
        PROPERTY_SCALE_Y => style_num(obj, LV_STYLE_TRANSFORM_SCALE_Y),
        _ => 0,
    }
}

/// Android rotates and scales about the view's centre; LVGL's default pivot
/// is the top-left corner. Set on every transform start — cheap, idempotent.
unsafe fn ensure_center_pivot(obj: *mut lv_obj_t) {
    let half = lv_pct(50);
    lv_obj_set_style_transform_pivot_x(obj, half, 0);
    lv_obj_set_style_transform_pivot_y(obj, half, 0);
}

// ── Starting ────────────────────────────────────────────────────────────────

/// Place `new_slot`. An existing slot for the same `(handle, property)` is
/// replaced when `replace_running` (immediate starts: re-issuing
/// `view.animate().alpha(...)` must not pile up old slots); a delayed start
/// passes `false` so it only displaces another *pending* slot for its key and
/// lets the running one continue until the delay expires.
unsafe fn insert_slot(new_slot: AnimSlot, replace_running: bool) {
    let slots = &mut *core::ptr::addr_of_mut!(ANIM_SLOTS);
    for slot in slots.iter_mut() {
        if slot.active
            && slot.handle == new_slot.handle
            && slot.property == new_slot.property
            && (replace_running || slot.from_pending)
        {
            *slot = new_slot;
            return;
        }
    }
    for slot in slots.iter_mut() {
        if !slot.active {
            *slot = new_slot;
            return;
        }
    }
    // Slot table full — silently drop. Apps that hit this are likely
    // animating dozens of widgets concurrently which isn't viable on
    // this platform anyway.
}

/// Begin an animation with an explicit `from`, both endpoints already in LVGL
/// units. Internal callers only (the system keyboard's slide-in); Java goes
/// through [`start_to`].
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
        delay_ms: 0,
        interpolator,
        from_pending: false,
        active: true,
    };
    unsafe { insert_slot(new_slot, true) }
}

/// Begin a to-only animation — Java `ViewPropertyAnimator.start()`. `to` is
/// in Android units; the implicit `from` is the view's current value, read
/// now for an immediate start or when `delay_ms` expires for a delayed one.
pub fn start_to(
    handle: i32,
    property: i32,
    to: f32,
    duration_ms: u32,
    delay_ms: u32,
    interpolator: i32,
) {
    let obj = handle_table::lookup(handle);
    if obj.is_null() {
        return; // stale handle — nothing to animate
    }
    let to = to_units(property, to);
    unsafe {
        if is_transform(property) {
            ensure_center_pivot(obj);
        }
        if duration_ms == 0 && delay_ms == 0 {
            apply_to_obj(obj, property, to);
            return;
        }
        let from_pending = delay_ms > 0;
        let from = if from_pending {
            0
        } else {
            read_current(obj, property)
        };
        insert_slot(
            AnimSlot {
                handle,
                property,
                from,
                to,
                duration_ms,
                elapsed_ms: 0,
                delay_ms,
                interpolator,
                from_pending,
                active: true,
            },
            !from_pending,
        );
    }
}

/// Register a Runnable to fire once `handle`'s animations complete (Android's
/// `withEndAction`). Replaces any existing action for the handle.
pub fn set_end_action(handle: i32, obj_ref: u16) {
    if handle == 0 {
        // 0 is the table's empty sentinel; an animator built on a stale or
        // failed handle would otherwise plant a (0, r) entry that the next
        // registration silently overwrites and the GC roots forever.
        return;
    }
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
/// no other slot for `handle` is still animating (pending ones included, so a
/// chain with a delayed leg fires once, at the end). Called when a slot retires.
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

/// Called once per frame from `LvglGfx::tick(ms)` — burns start delays,
/// advances each running slot, applies the interpolated value, and clears
/// slots whose deadline has passed.
pub fn tick(ms: u32) {
    // Handles whose slot retired this tick — checked for end-action firing
    // after the main loop so we don't read ANIM_SLOTS while iterating it `mut`.
    let mut retired: [i32; MAX_ANIMATIONS] = [0; MAX_ANIMATIONS];
    let mut retired_len = 0usize;
    unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(ANIM_SLOTS);
        for i in 0..MAX_ANIMATIONS {
            let mut slot = slots[i];
            if !slot.active {
                continue;
            }
            // Burn the start delay first; a tick that crosses the boundary
            // carries its remainder into the animation proper.
            let mut step = ms;
            if slot.delay_ms > 0 {
                if slot.delay_ms > step {
                    slot.delay_ms -= step;
                    slots[i] = slot;
                    continue;
                }
                step -= slot.delay_ms;
                slot.delay_ms = 0;
            }
            if slot.from_pending {
                // The delay just expired: start from the value the view has
                // *now*, and take over from any still-running animation of the
                // same property (kept alive until here only to be superseded).
                slot.from_pending = false;
                let obj = handle_table::lookup(slot.handle);
                if !obj.is_null() {
                    slot.from = read_current(obj, slot.property);
                }
                for (j, other) in slots.iter_mut().enumerate() {
                    if j != i
                        && other.active
                        && other.handle == slot.handle
                        && other.property == slot.property
                    {
                        *other = EMPTY_ANIM;
                    }
                }
            }
            slot.elapsed_ms = slot.elapsed_ms.saturating_add(step);
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
            slots[i] = slot;
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
    unsafe { apply_to_obj(obj, property, value) }
}

unsafe fn apply_to_obj(obj: *mut lv_obj_t, property: i32, value: i32) {
    match property {
        PROPERTY_ALPHA => {
            let alpha = value.clamp(0, 255) as u8;
            lv_obj_set_style_opa(obj, alpha, 0);
        }
        PROPERTY_X => lv_obj_set_x(obj, value),
        PROPERTY_Y => lv_obj_set_y(obj, value),
        PROPERTY_TRANSLATION_X => lv_obj_set_style_translate_x(obj, value, 0),
        PROPERTY_TRANSLATION_Y => lv_obj_set_style_translate_y(obj, value, 0),
        PROPERTY_ROTATION => lv_obj_set_style_transform_rotation(obj, value, 0),
        PROPERTY_SCALE_X => lv_obj_set_style_transform_scale_x(obj, value.max(0), 0),
        PROPERTY_SCALE_Y => lv_obj_set_style_transform_scale_y(obj, value.max(0), 0),
        _ => {} // unknown property — silently ignore
    }
}

// ── View accessors (`View.setTranslationX` & co) ───────────────────────────

/// Set `property` immediately, Android units. Any running animation of the
/// same property keeps going and will overwrite this on its next frame —
/// as on Android, where a setter during an animation is a one-frame blip.
pub fn set_property(handle: i32, property: i32, value: f32) {
    let obj = handle_table::lookup(handle);
    if obj.is_null() {
        return; // stale handle — mutating a destroyed View is a no-op
    }
    unsafe {
        if is_transform(property) {
            ensure_center_pivot(obj);
        }
        apply_to_obj(obj, property, to_units(property, value));
    }
}

/// The property's current value, Android units. A stale handle reports the
/// property's identity (0, or 1.0 for alpha and scale).
pub fn get_property(handle: i32, property: i32) -> f32 {
    let obj = handle_table::lookup(handle);
    let units = if obj.is_null() {
        match property {
            PROPERTY_ALPHA => 255,
            PROPERTY_SCALE_X | PROPERTY_SCALE_Y => LV_SCALE_NONE,
            _ => 0,
        }
    } else {
        unsafe { read_current(obj, property) }
    };
    from_units(property, units)
}
