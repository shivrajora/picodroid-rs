//! Socket handle ↔ Java `handle` (i32) conversion.
//!
//! On 32-bit MCU: zero-cost bit-preserving cast (`*mut c_void` is 32 bits).
//! On 64-bit sim: static indirection table (same pattern as graphics handle_table).

use core::ffi::c_void;

// ── MCU: direct cast (zero overhead) ─────────────────────────────────────────

#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn register(ptr: *mut c_void) -> i32 {
    ptr as u32 as i32
}

#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn lookup(id: i32) -> *mut c_void {
    id as u32 as *mut c_void
}

#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn remove(_id: i32) {}

#[cfg(not(feature = "sim"))]
#[inline(always)]
pub fn reset() {}

// ── Sim: indirection table ───────────────────────────────────────────────────

#[cfg(feature = "sim")]
const MAX_HANDLES: usize = 32;

#[cfg(feature = "sim")]
static mut HANDLES: [*mut c_void; MAX_HANDLES] = [core::ptr::null_mut(); MAX_HANDLES];

#[cfg(feature = "sim")]
static mut COUNT: usize = 0;

#[cfg(feature = "sim")]
pub fn register(ptr: *mut c_void) -> i32 {
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        COUNT += 1;
        assert!(COUNT < MAX_HANDLES, "socket handle table full");
        HANDLES[COUNT] = ptr;
        COUNT as i32
    }
}

#[cfg(feature = "sim")]
pub fn lookup(id: i32) -> *mut c_void {
    if id <= 0 || (id as usize) >= MAX_HANDLES {
        return core::ptr::null_mut();
    }
    unsafe { HANDLES[id as usize] }
}

#[cfg(feature = "sim")]
pub fn remove(id: i32) {
    if id > 0 && (id as usize) < MAX_HANDLES {
        unsafe {
            HANDLES[id as usize] = core::ptr::null_mut();
        }
    }
}

#[cfg(feature = "sim")]
#[allow(dead_code)]
pub fn reset() {
    unsafe {
        let handles = &raw mut HANDLES;
        for i in 0..MAX_HANDLES {
            (*handles)[i] = core::ptr::null_mut();
        }
        COUNT = 0;
    }
}
