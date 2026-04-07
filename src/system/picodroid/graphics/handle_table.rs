//! LVGL object handle ↔ Java `nativeHandle` (i32) conversion.
//!
//! Java's `nativeHandle` field is 32-bit.  On the 32-bit RP2040/RP2350
//! `lv_obj_t*` is also 32 bits, so the register/lookup functions are
//! zero-cost bit-preserving casts — no table, no memory, no overhead.
//!
//! On 64-bit sim hosts the upper 32 bits of a pointer would be lost by a
//! direct cast, so we maintain a small static indirection table instead.

use crate::lvgl_ffi::lv_obj_t;

// ── MCU: direct cast (zero overhead) ─────────────────────────────────────────

/// Store a pointer as a Java `nativeHandle`.  On MCU: bit-preserving cast.
#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn register(ptr: *mut lv_obj_t) -> i32 {
    ptr as u32 as i32
}

/// Recover a pointer from a Java `nativeHandle`.  On MCU: bit-preserving cast.
#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn lookup(id: i32) -> *mut lv_obj_t {
    id as u32 as *mut lv_obj_t
}

/// No-op on MCU — there is no table to clear.
#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn reset() {}

// ── Sim: indirection table ────────────────────────────────────────────────────

#[cfg(feature = "sim")]
const MAX_HANDLES: usize = 128;

#[cfg(feature = "sim")]
static mut HANDLES: [*mut lv_obj_t; MAX_HANDLES] = [core::ptr::null_mut(); MAX_HANDLES];

#[cfg(feature = "sim")]
static mut COUNT: usize = 0;

/// Register a pointer and return a 1-based integer ID (Java `nativeHandle`).
/// Returns 0 if `ptr` is null.
#[cfg(feature = "sim")]
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
#[cfg(feature = "sim")]
pub fn lookup(id: i32) -> *mut lv_obj_t {
    if id <= 0 || (id as usize) >= MAX_HANDLES {
        return core::ptr::null_mut();
    }
    unsafe { HANDLES[id as usize] }
}

/// Clear all registrations (call between app runs).
#[cfg(feature = "sim")]
pub fn reset() {
    unsafe {
        let handles = &raw mut HANDLES;
        for i in 0..MAX_HANDLES {
            (*handles)[i] = core::ptr::null_mut();
        }
        COUNT = 0;
    }
}
