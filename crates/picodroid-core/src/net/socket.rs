// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.Socket.

use alloc::format;

use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::helpers::{
    extract_handle, extract_socket_ptr, throw_io_exception, throw_net_exception, NetOpCtx,
};
use super::socket_table;

/// Max bytes per send/recv call — stack-allocated intermediate buffer.
const BUF_SIZE: usize = 256;

/// Socket.nativeCreate() — create a TCP socket, return handle.
pub fn native_create(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let ptr = crate::hal::net::tcp_socket().map_err(|e| {
        throw_io_exception(
            objects,
            strings,
            &format!("socket create failed (err {})", e.raw),
        )
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

/// Socket.connect(int addr, int port)
///
/// Failure surfaces as the typed `java.net` exception Android apps expect:
/// `ConnectException` on refusal, `SocketTimeoutException` on timeout,
/// `IOException` otherwise — an unreachable host is an app-visible
/// condition, not a JVM fault.
pub fn connect_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, strings, fields::socket::HANDLE)?;
    let addr = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let port = match args.get(2) {
        Some(Value::Int(v)) => *v as u16,
        _ => return Err(JvmError::InvalidReference),
    };
    crate::hal::net::tcp_connect(ptr, addr, port)
        .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Connect))?;
    Ok(None)
}

/// Socket.send(byte[] data, int offset, int len) -> int
///
/// Failure throws (`SocketException` on a reset connection), matching
/// Android's `OutputStream.write` contract — never a silent `-1`.
pub fn send_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, strings, fields::socket::HANDLE)?;
    let arr_idx = match args.get(1) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let offset = match args.get(2) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(3) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };

    // Copy from JVM array into stack buffer.
    let send_len = len.min(BUF_SIZE);
    let mut buf = [0u8; BUF_SIZE];
    for (i, b) in buf.iter_mut().enumerate().take(send_len) {
        *b = arrays
            .load(arr_idx, offset + i)
            .ok_or(JvmError::ArrayIndexOutOfBounds)? as u8;
    }

    let n = crate::hal::net::tcp_send(ptr, &buf[..send_len])
        .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Send))?;
    Ok(Some(Value::Int(n as i32)))
}

/// Socket.recv(byte[] buf, int offset, int len) -> int
///
/// Returns `-1` **only** for orderly end-of-stream. A receive-timeout
/// expiry throws `SocketTimeoutException`; other failures throw
/// `SocketException`/`IOException`.
pub fn recv_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, strings, fields::socket::HANDLE)?;
    let arr_idx = match args.get(1) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let offset = match args.get(2) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(3) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    if len == 0 {
        // A zero-length read must not consult the HAL: its `Ok(0)` return
        // means EOF, which a `recv(buf, off, 0)` caller has not reached.
        return Ok(Some(Value::Int(0)));
    }

    let recv_len = len.min(BUF_SIZE);
    let mut buf = [0u8; BUF_SIZE];
    let n = crate::hal::net::tcp_recv(ptr, &mut buf[..recv_len])
        .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Recv))?;
    if n == 0 {
        return Ok(Some(Value::Int(-1)));
    }
    // Copy received bytes into JVM array.
    for (i, &b) in buf.iter().enumerate().take(n) {
        arrays
            .store(arr_idx, offset + i, b as i32)
            .ok_or(JvmError::ArrayIndexOutOfBounds)?;
    }
    Ok(Some(Value::Int(n as i32)))
}

/// Socket.setTimeout(int millis)
pub fn set_timeout_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, strings, fields::socket::HANDLE)?;
    let ms = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    crate::hal::net::set_recv_timeout(ptr, ms);
    Ok(None)
}

/// Socket.close()
pub fn close_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_handle(args, objects, fields::socket::HANDLE)?;
    let ptr = socket_table::lookup(handle);
    if !ptr.is_null() {
        crate::hal::net::close(ptr);
        socket_table::remove(handle);
    }
    Ok(None)
}
