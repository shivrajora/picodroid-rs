// SPDX-License-Identifier: GPL-3.0-only
//! Socket handle ↔ Java `handle` (i32) conversion.
//!
//! Thin wrapper over [`PtrTable`](super::ptr_table::PtrTable) — one
//! slot-reusing implementation for every pointer width. (Previously the
//! 32-bit arm cast the raw pointer to the handle with a no-op `remove`,
//! so close-then-use dereferenced a freed socket; see `ptr_table`.)

use core::ffi::c_void;

use super::ptr_table::PtrTable;

const MAX_HANDLES: usize = 32;

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
