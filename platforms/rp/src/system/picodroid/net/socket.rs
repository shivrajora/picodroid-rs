// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.Socket.

use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::helpers::{extract_handle, extract_socket_ptr};
use super::socket_table;

/// Max bytes per send/recv call — stack-allocated intermediate buffer.
const BUF_SIZE: usize = 256;

/// Socket.nativeCreate() — create a TCP socket, return handle.
pub fn native_create() -> Result<Option<Value>, JvmError> {
    let ptr = crate::hal::net::tcp_socket().map_err(|_| JvmError::InvalidReference)?;
    let handle = socket_table::register(ptr);
    Ok(Some(Value::Int(handle)))
}

/// Socket.connect(int addr, int port)
pub fn connect_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::socket::HANDLE)?;
    let addr = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let port = match args.get(2) {
        Some(Value::Int(v)) => *v as u16,
        _ => return Err(JvmError::InvalidReference),
    };
    crate::hal::net::tcp_connect(ptr, addr, port).map_err(|_| JvmError::InvalidReference)?;
    Ok(None)
}

/// Socket.send(byte[] data, int offset, int len) -> int
pub fn send_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::socket::HANDLE)?;
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
    for i in 0..send_len {
        buf[i] = arrays
            .load(arr_idx, offset + i)
            .ok_or(JvmError::ArrayIndexOutOfBounds)? as u8;
    }

    match crate::hal::net::tcp_send(ptr, &buf[..send_len]) {
        Ok(n) => Ok(Some(Value::Int(n as i32))),
        Err(_) => Ok(Some(Value::Int(-1))),
    }
}

/// Socket.recv(byte[] buf, int offset, int len) -> int
pub fn recv_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::socket::HANDLE)?;
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

    let recv_len = len.min(BUF_SIZE);
    let mut buf = [0u8; BUF_SIZE];
    match crate::hal::net::tcp_recv(ptr, &mut buf[..recv_len]) {
        Ok(n) => {
            // Copy received bytes into JVM array.
            for i in 0..n {
                arrays.store(arr_idx, offset + i, buf[i] as i32);
            }
            Ok(Some(Value::Int(n as i32)))
        }
        Err(_) => Ok(Some(Value::Int(-1))),
    }
}

/// Socket.setTimeout(int millis)
pub fn set_timeout_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::socket::HANDLE)?;
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
