// SPDX-License-Identifier: GPL-3.0-only
//! HTTP connection handle ↔ Java `handle` (i32) conversion.
//!
//! Same wrapper shape as [`socket_table`](super::socket_table), over the
//! shared slot-reusing [`PtrTable`](super::ptr_table::PtrTable).

use core::ffi::c_void;

use super::ptr_table::PtrTable;

const MAX_HANDLES: usize = 16;

static TABLE: PtrTable<MAX_HANDLES> = PtrTable::new();

pub fn register(ptr: *mut c_void) -> i32 {
    TABLE.register(ptr)
}

pub fn lookup(id: i32) -> *mut c_void {
    TABLE.lookup(id)
}

pub fn remove(id: i32) {
    TABLE.remove(id)
}
