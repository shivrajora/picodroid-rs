// SPDX-License-Identifier: GPL-3.0-only
//! LVGL object handle ↔ Java `nativeHandle` (i32) conversion.
//!
//! Java's `nativeHandle` field is 32-bit.  On 32-bit targets (RP2040/RP2350)
//! `lv_obj_t*` is also 32 bits, so the register/lookup functions are
//! zero-cost bit-preserving casts — no table, no memory, no overhead.
//!
//! On 64-bit targets the upper 32 bits of a pointer would be lost by a
//! direct cast, so we maintain a small static indirection table instead.

use crate::lvgl_ffi::lv_obj_t;

// ── 32-bit: direct cast (zero overhead) ──────────────────────────────────────

/// Store a pointer as a Java `nativeHandle`.  On 32-bit: bit-preserving cast.
#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub fn register(ptr: *mut lv_obj_t) -> i32 {
    ptr as u32 as i32
}

/// Recover a pointer from a Java `nativeHandle`.  On 32-bit: bit-preserving cast.
#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub fn lookup(id: i32) -> *mut lv_obj_t {
    id as u32 as *mut lv_obj_t
}

/// No-op on 32-bit — there is no table to clear.
#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub fn reset() {}

// ── 64-bit: indirection table ─────────────────────────────────────────────────

// NOTE: `register` is monotonic — it never *reclaims* a slot, but it does
// *invalidate* one: a per-object `LV_EVENT_DELETE` hook stamps HANDLES[id] with
// the [`DELETED`] sentinel when LVGL frees the object (directly, or as a
// descendant of a `lv_obj_delete` / `lv_obj_clean` / screen switch). So a stale
// Java `nativeHandle` resolves to null via `lookup` and view ops no-op, instead
// of dereferencing freed memory (the History-screen use-after-free). Slots are
// not reused — that keeps the "stale handle → null" safety property (reuse
// would alias a new object), so the ceiling is still on *cumulative* widget
// creations over a run, not concurrently-live widgets. A normal app builds a
// bounded UI once and stays well under any sane cap, but the `graphicsbench`
// example deliberately churns hundreds of widgets, so the host table is sized
// generously. 64-bit / sim-only: the 32-bit hardware path above is a zero-cost
// cast with no table and no limit. 4096 * 8 B = 32 KiB of host RAM, negligible
// in the simulator. Reclaiming freed slots is a separate tracked follow-up (it
// needs generational ids to stay safe).
//
// This null-on-delete net is a 64-bit luxury the 32-bit hardware path can't
// have: there a `nativeHandle` *is* the raw `lv_obj_t*`, so a deleted handle
// *dangles* into freed LVGL memory rather than nulling, and any subsystem that
// outlives the view (e.g. the animation engine) dereferences it — typically
// hanging in LVGL's `LV_ASSERT_NULL`. To surface those hardware-only bugs in
// the sim, set `PICODROID_HANDLE_SANITIZER=1`: `lookup` then aborts loudly with
// a backtrace on any use-after-delete instead of silently returning null. See
// [`sanitizer_enabled`].
#[cfg(target_pointer_width = "64")]
const MAX_HANDLES: usize = 4096;

#[cfg(target_pointer_width = "64")]
static mut HANDLES: [*mut lv_obj_t; MAX_HANDLES] = [core::ptr::null_mut(); MAX_HANDLES];

#[cfg(target_pointer_width = "64")]
static mut COUNT: usize = 0;

/// Sentinel stamped into a slot by [`handle_delete_cb`] when its object is
/// deleted. Kept distinct from a never-registered slot (`null`) so the handle
/// sanitizer can tell a use-after-delete apart from a genuinely unknown id.
/// Never a valid `lv_obj_t*` (no object lives at `usize::MAX`).
#[cfg(target_pointer_width = "64")]
const DELETED: *mut lv_obj_t = usize::MAX as *mut lv_obj_t;

/// Whether the handle use-after-delete sanitizer is enabled. Read once from
/// `PICODROID_HANDLE_SANITIZER` (`1`/`on`/`true`/`yes`) and cached. Off by
/// default, so normal sim/test runs keep the silent-null behaviour; turn it on
/// in CI or a debugging run to make every stale-handle lookup abort with a
/// backtrace pointing at the offending caller — the bug class the 64-bit table
/// otherwise hides from the sim (see the module note).
#[cfg(target_pointer_width = "64")]
fn sanitizer_enabled() -> bool {
    use core::sync::atomic::{AtomicU8, Ordering};
    // 0 = unread, 1 = off, 2 = on.
    static STATE: AtomicU8 = AtomicU8::new(0);
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = std::env::var("PICODROID_HANDLE_SANITIZER")
                .map(|v| matches!(v.as_str(), "1" | "on" | "true" | "yes"))
                .unwrap_or(false);
            STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Loud, deterministic stop for a use-after-delete handle access. Captures a
/// backtrace unconditionally — the whole point is to show *where* the stale
/// lookup came from, regardless of `RUST_BACKTRACE`.
#[cfg(target_pointer_width = "64")]
#[cold]
#[inline(never)]
fn report_use_after_delete(id: usize) -> ! {
    let backtrace = std::backtrace::Backtrace::force_capture();
    panic!(
        "handle-sanitizer: nativeHandle {id} was used after its view was deleted. \
         This is a use-after-free the 32-bit hardware handle table exposes (there a \
         deleted handle dangles into freed LVGL memory and typically hangs in \
         LV_ASSERT_NULL), while the 64-bit table silently no-ops. Fix the owner to \
         cancel/deregister the handle on delete.\noffending call site:\n{backtrace}"
    );
}

/// Register a pointer and return a 1-based integer ID (Java `nativeHandle`).
/// Returns 0 if `ptr` is null.
#[cfg(target_pointer_width = "64")]
pub fn register(ptr: *mut lv_obj_t) -> i32 {
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        COUNT += 1;
        assert!(COUNT < MAX_HANDLES, "LVGL handle table full");
        let id = COUNT;
        HANDLES[id] = ptr;
        // Invalidate this slot when LVGL deletes the object. The slot id rides
        // in the event user_data so the delete hook clears the exact entry in
        // O(1). See the module note above on why slots aren't reused.
        crate::lvgl_ffi::lv_obj_add_event_cb(
            ptr,
            Some(handle_delete_cb),
            crate::lvgl_ffi::LV_EVENT_DELETE,
            id as *mut core::ffi::c_void,
        );
        id as i32
    }
}

/// `LV_EVENT_DELETE` hook installed by [`register`]: stamps the deleted object's
/// handle slot with the [`DELETED`] sentinel so a stale `nativeHandle` resolves
/// to null (or, with the sanitizer on, aborts) rather than freed memory.
/// Guarded against a slot that no longer references this object.
#[cfg(target_pointer_width = "64")]
unsafe extern "C" fn handle_delete_cb(e: *mut crate::lvgl_ffi::lv_event_t) {
    let id = unsafe { crate::lvgl_ffi::lv_event_get_user_data(e) } as usize;
    if id == 0 || id >= MAX_HANDLES {
        return;
    }
    let obj = unsafe { crate::lvgl_ffi::lv_event_get_target_obj(e) };
    let handles = &raw mut HANDLES;
    unsafe {
        if (*handles)[id] == obj {
            (*handles)[id] = DELETED;
        }
    }
}

/// Look up the full pointer for a previously registered ID. Returns null for an
/// ID ≤ 0, an out-of-range ID, or a slot whose view has been deleted — unless
/// the handle sanitizer is enabled, in which case a deleted-slot lookup aborts
/// loudly (see [`sanitizer_enabled`]).
#[cfg(target_pointer_width = "64")]
pub fn lookup(id: i32) -> *mut lv_obj_t {
    if id <= 0 || (id as usize) >= MAX_HANDLES {
        return core::ptr::null_mut();
    }
    let ptr = unsafe { HANDLES[id as usize] };
    if ptr == DELETED {
        if sanitizer_enabled() {
            report_use_after_delete(id as usize);
        }
        return core::ptr::null_mut();
    }
    ptr
}

/// Clear all registrations (call between app runs).
#[cfg(target_pointer_width = "64")]
pub fn reset() {
    unsafe {
        let handles = &raw mut HANDLES;
        for i in 0..MAX_HANDLES {
            (*handles)[i] = core::ptr::null_mut();
        }
        COUNT = 0;
    }
}
