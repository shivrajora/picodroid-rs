//! Portable LVGL object handle table.
//!
//! Java's `nativeHandle` field is an `int` (32 bits), but on 64-bit hosts
//! `lv_obj_t*` is 64 bits.  This module stores full pointers in a fixed-size
//! static table and hands out small integer IDs instead, keeping the Java API
//! unchanged.
//!
//! On 32-bit embedded targets the table overhead is minimal — it still works
//! correctly even though a direct cast would also have sufficed.

use crate::lvgl_ffi::lv_obj_t;

const MAX_HANDLES: usize = 128;

static mut HANDLES: [*mut lv_obj_t; MAX_HANDLES] = [core::ptr::null_mut(); MAX_HANDLES];
static mut COUNT: usize = 0;

/// Register a pointer and return a 1-based integer ID (Java `nativeHandle`).
/// Returns 0 if `ptr` is null.
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
pub fn lookup(id: i32) -> *mut lv_obj_t {
    if id <= 0 || (id as usize) >= MAX_HANDLES {
        return core::ptr::null_mut();
    }
    unsafe { HANDLES[id as usize] }
}

/// Clear all registrations (call between app runs).
pub fn reset() {
    unsafe {
        let handles = &raw mut HANDLES;
        for i in 0..MAX_HANDLES {
            (*handles)[i] = core::ptr::null_mut();
        }
        COUNT = 0;
    }
}
