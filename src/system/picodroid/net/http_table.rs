//! HTTP connection handle ↔ Java `handle` (i32) conversion.
//!
//! Same dual-mode pattern as [`socket_table`](super::socket_table): on 32-bit
//! targets the pointer fits in an `i32`; on 64-bit (sim) we use a static
//! indirection table.

use core::ffi::c_void;

// ── 32-bit: direct cast (zero overhead) ──────────────────────────────────────

#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub fn register(ptr: *mut c_void) -> i32 {
    ptr as u32 as i32
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub fn lookup(id: i32) -> *mut c_void {
    id as u32 as *mut c_void
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub fn remove(_id: i32) {}

// ── 64-bit: indirection table ────────────────────────────────────────────────

#[cfg(target_pointer_width = "64")]
const MAX_HANDLES: usize = 16;

#[cfg(target_pointer_width = "64")]
static mut HANDLES: [*mut c_void; MAX_HANDLES] = [core::ptr::null_mut(); MAX_HANDLES];

#[cfg(target_pointer_width = "64")]
static mut COUNT: usize = 0;

#[cfg(target_pointer_width = "64")]
pub fn register(ptr: *mut c_void) -> i32 {
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        COUNT += 1;
        assert!(COUNT < MAX_HANDLES, "http handle table full");
        HANDLES[COUNT] = ptr;
        COUNT as i32
    }
}

#[cfg(target_pointer_width = "64")]
pub fn lookup(id: i32) -> *mut c_void {
    if id <= 0 || (id as usize) >= MAX_HANDLES {
        return core::ptr::null_mut();
    }
    unsafe { HANDLES[id as usize] }
}

#[cfg(target_pointer_width = "64")]
pub fn remove(id: i32) {
    if id > 0 && (id as usize) < MAX_HANDLES {
        unsafe {
            HANDLES[id as usize] = core::ptr::null_mut();
        }
    }
}
