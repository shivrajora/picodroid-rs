// SPDX-License-Identifier: GPL-3.0-only
//! Shared helper functions for picodroid.net native methods.

use core::ffi::c_void;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::socket_table;

/// Allocate a `java/io/IOException` carrying `msg` and wrap it as a thrown
/// Java exception. Alloc-by-name with exact-name catch, the pattern
/// established for NumberFormatException/UnsupportedOperationException — the
/// class needs no .class file, and `Throwable.getMessage()` surfaces the
/// message via the ObjectHeap side table.
///
/// Use this for genuine I/O failures (unreachable host, reset connection…)
/// so apps get the catchable Android contract; keep `InvalidReference` for
/// malformed native arguments, which are bugs, not I/O conditions.
pub fn throw_io_exception(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    msg: &str,
) -> JvmError {
    match objects.alloc("java/io/IOException") {
        Some(idx) => {
            if let Some(midx) = strings.intern_dyn(msg.as_bytes()) {
                objects.register_exception_message(idx, midx);
            }
            JvmError::Exception(idx)
        }
        None => JvmError::StackOverflow,
    }
}

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
