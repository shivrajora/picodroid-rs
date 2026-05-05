// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.ServerSocket.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::helpers::{extract_handle, extract_socket_ptr};
use super::socket_table;

/// ServerSocket.nativeListen(int port) — create, bind, listen; return handle.
pub fn native_listen(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let port = match args.first() {
        Some(Value::Int(v)) => *v as u16,
        _ => return Err(JvmError::InvalidReference),
    };
    let ptr = crate::hal::net::tcp_socket().map_err(|_| JvmError::InvalidReference)?;
    crate::hal::net::tcp_listen(ptr, port).map_err(|_| {
        crate::hal::net::close(ptr);
        JvmError::InvalidReference
    })?;
    let handle = socket_table::register(ptr);
    Ok(Some(Value::Int(handle)))
}

/// ServerSocket.accept() -> Socket
pub fn accept_native(args: &[Value], objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::server_socket::HANDLE)?;
    let client_ptr = crate::hal::net::tcp_accept(ptr).map_err(|_| JvmError::InvalidReference)?;
    let client_handle = socket_table::register(client_ptr);

    // Allocate a new Socket object and set its handle field.
    let obj_idx = objects
        .alloc("picodroid/net/Socket")
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::socket::HANDLE, Value::Int(client_handle))
        .ok_or(JvmError::StackOverflow)?;

    Ok(Some(Value::ObjectRef(obj_idx)))
}

/// ServerSocket.close()
pub fn close_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_handle(args, objects, fields::server_socket::HANDLE)?;
    let ptr = socket_table::lookup(handle);
    if !ptr.is_null() {
        crate::hal::net::close(ptr);
        socket_table::remove(handle);
    }
    Ok(None)
}
