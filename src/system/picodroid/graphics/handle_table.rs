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

#[cfg(target_pointer_width = "64")]
const MAX_HANDLES: usize = 128;

#[cfg(target_pointer_width = "64")]
static mut HANDLES: [*mut lv_obj_t; MAX_HANDLES] = [core::ptr::null_mut(); MAX_HANDLES];

#[cfg(target_pointer_width = "64")]
static mut COUNT: usize = 0;

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
        HANDLES[COUNT] = ptr;
        COUNT as i32
    }
}

/// Look up the full pointer for a previously registered ID.
/// Returns null for ID ≤ 0 or out-of-range IDs.
#[cfg(target_pointer_width = "64")]
pub fn lookup(id: i32) -> *mut lv_obj_t {
    if id <= 0 || (id as usize) >= MAX_HANDLES {
        return core::ptr::null_mut();
    }
    unsafe { HANDLES[id as usize] }
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
