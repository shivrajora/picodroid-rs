// SPDX-License-Identifier: GPL-3.0-only
//! Shared helper functions for picodroid.net native methods.

use core::ffi::c_void;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::socket_table;

/// Extract `this` object index from args[0].
pub fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Read the socket handle field from `this` and look up the raw pointer.
pub fn extract_socket_ptr(
    args: &[Value],
    objects: &ObjectHeap,
    handle_field: usize,
) -> Result<*mut c_void, JvmError> {
    let idx = extract_obj_idx(args)?;
    let handle = match objects.get_field(idx, handle_field) {
        Some(Value::Int(h)) => h,
        _ => return Err(JvmError::InvalidReference),
    };
    let ptr = socket_table::lookup(handle);
    if ptr.is_null() {
        return Err(JvmError::InvalidReference);
    }
    Ok(ptr)
}

/// Read the socket handle (i32) from `this` object's field.
pub fn extract_handle(args: &[Value], objects: &ObjectHeap, field: usize) -> Result<i32, JvmError> {
    let idx = extract_obj_idx(args)?;
    match objects.get_field(idx, field) {
        Some(Value::Int(h)) => Ok(h),
        _ => Err(JvmError::InvalidReference),
    }
}
