// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.DatagramSocket.

use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::helpers::{extract_handle, extract_socket_ptr};
use super::socket_table;

const BUF_SIZE: usize = 256;

/// DatagramSocket.nativeCreate(int localPort) -> int handle
pub fn native_create(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let port = match args.first() {
        Some(Value::Int(v)) => *v as u16,
        _ => return Err(JvmError::InvalidReference),
    };
    let ptr = crate::hal::net::udp_socket(port).map_err(|_| JvmError::InvalidReference)?;
    let handle = socket_table::register(ptr);
    Ok(Some(Value::Int(handle)))
}

/// DatagramSocket.send(DatagramPacket packet)
pub fn send_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::datagram_socket::HANDLE)?;

    // args[1] = DatagramPacket object
    let pkt_idx = match args.get(1) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };

    // Read packet fields.
    let arr_idx = match objects.get_field(pkt_idx, fields::datagram_packet::DATA) {
        Some(Value::ArrayRef(idx)) => idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let length = match objects.get_field(pkt_idx, fields::datagram_packet::LENGTH) {
        Some(Value::Int(v)) => v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let addr = match objects.get_field(pkt_idx, fields::datagram_packet::ADDRESS) {
        Some(Value::Int(v)) => v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let port = match objects.get_field(pkt_idx, fields::datagram_packet::PORT) {
        Some(Value::Int(v)) => v as u16,
        _ => return Err(JvmError::InvalidReference),
    };

    let send_len = length.min(BUF_SIZE);
    let mut buf = [0u8; BUF_SIZE];
    for i in 0..send_len {
        buf[i] = arrays
            .load(arr_idx, i)
            .ok_or(JvmError::ArrayIndexOutOfBounds)? as u8;
    }

    crate::hal::net::udp_sendto(ptr, &buf[..send_len], addr, port)
        .map_err(|_| JvmError::InvalidReference)?;
    Ok(None)
}

/// DatagramSocket.receive(DatagramPacket packet)
pub fn receive_native(
    args: &[Value],
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::datagram_socket::HANDLE)?;

    let pkt_idx = match args.get(1) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };

    // Read the packet's data array ref to know where to write.
    let arr_idx = match objects.get_field(pkt_idx, fields::datagram_packet::DATA) {
        Some(Value::ArrayRef(idx)) => idx,
        _ => return Err(JvmError::InvalidReference),
    };

    let mut buf = [0u8; BUF_SIZE];
    let (n, src_addr, src_port) =
        crate::hal::net::udp_recvfrom(ptr, &mut buf).map_err(|_| JvmError::InvalidReference)?;

    // Copy received bytes into the packet's data array.
    let copy_len: usize = n.min(BUF_SIZE);
    for i in 0..copy_len {
        arrays.store(arr_idx, i, buf[i] as i32);
    }

    // Update packet fields.
    objects
        .set_field(
            pkt_idx,
            fields::datagram_packet::LENGTH,
            Value::Int(n as i32),
        )
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(
            pkt_idx,
            fields::datagram_packet::ADDRESS,
            Value::Int(src_addr as i32),
        )
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(
            pkt_idx,
            fields::datagram_packet::PORT,
            Value::Int(src_port as i32),
        )
        .ok_or(JvmError::StackOverflow)?;

    Ok(None)
}

/// DatagramSocket.setTimeout(int millis)
pub fn set_timeout_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let ptr = extract_socket_ptr(args, objects, fields::datagram_socket::HANDLE)?;
    let ms = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    crate::hal::net::set_recv_timeout(ptr, ms);
    Ok(None)
}

/// DatagramSocket.close()
pub fn close_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_handle(args, objects, fields::datagram_socket::HANDLE)?;
    let ptr = socket_table::lookup(handle);
    if !ptr.is_null() {
        crate::hal::net::close(ptr);
        socket_table::remove(handle);
    }
    Ok(None)
}
