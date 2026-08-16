// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.ServerSocket.

use alloc::format;

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::helpers::{
    extract_handle, extract_socket_ptr, throw_io_exception, throw_net_exception, NetOpCtx,
};
use super::socket_table;

/// ServerSocket.nativeListen(int port) — create, bind, listen; return handle.
///
/// A bind conflict throws `java/net/BindException` ("Address already in
/// use"), other failures `IOException` — matching `new ServerSocket(port)`.
pub fn native_listen(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let port = match args.first() {
        Some(Value::Int(v)) => *v as u16,
        _ => return Err(JvmError::InvalidReference),
    };
    let ptr = crate::hal::net::tcp_socket().map_err(|e| {
        throw_io_exception(
            objects,
            strings,
            &format!("socket create failed (err {})", e.raw),
        )
    })?;
    crate::hal::net::tcp_listen(ptr, port).map_err(|e| {
        crate::hal::net::close(ptr);
        throw_net_exception(objects, strings, e, NetOpCtx::Bind)
    })?;
    let handle = socket_table::register(ptr);
    if handle == 0 {
        crate::hal::net::close(ptr);
        return Err(throw_io_exception(
            objects,
            strings,
            "too many open sockets",
        ));
    }
    Ok(Some(Value::Int(handle)))
}

/// ServerSocket.accept() -> Socket
///
/// A `setTimeout` expiry throws `java/net/SocketTimeoutException`
/// ("Accept timed out"), matching `java.net.ServerSocket.accept`.
pub fn accept_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, strings, fields::server_socket::HANDLE)?;
    let client_ptr = crate::hal::net::tcp_accept(ptr)
        .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Accept))?;
    let client_handle = socket_table::register(client_ptr);
    if client_handle == 0 {
        crate::hal::net::close(client_ptr);
        return Err(throw_io_exception(
            objects,
            strings,
            "too many open sockets",
        ));
    }

    // Allocate a new Socket object and set its handle field.
    let obj_idx = objects
        .alloc(crate::shrink_names::shrink_class("picodroid/net/Socket"))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::socket::HANDLE, Value::Int(client_handle))
        .ok_or(JvmError::StackOverflow)?;

    Ok(Some(Value::ObjectRef(obj_idx)))
}

/// ServerSocket.setSoTimeout(int millis)
pub fn set_so_timeout_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, strings, fields::server_socket::HANDLE)?;
    let ms = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    crate::hal::net::set_recv_timeout(ptr, ms);
    Ok(None)
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
