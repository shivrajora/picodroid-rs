// SPDX-License-Identifier: GPL-3.0-only
//! HTTP connection handle ↔ Java `handle` (i32) conversion.
//!
//! Same wrapper shape as [`socket_table`](super::socket_table), over the
//! shared slot-reusing [`PtrTable`](super::ptr_table::PtrTable).

use core::ffi::c_void;

use super::ptr_table::PtrTable;
use crate::util::local::Core0;

const MAX_HANDLES: usize = 16;

// SAFETY: every caller is a HTTP connection native, which runs on a core-0 JVM task
// with the JVM run lock held (the table is never touched inside an
// `unlocked()` scope).
static TABLE: Core0<PtrTable<MAX_HANDLES>> = unsafe { Core0::new(PtrTable::new()) };

pub fn register(ptr: *mut c_void) -> i32 {
    TABLE.register(ptr)
}

pub fn lookup(id: i32) -> *mut c_void {
    TABLE.lookup(id)
}

pub fn remove(id: i32) {
    TABLE.remove(id)
}
