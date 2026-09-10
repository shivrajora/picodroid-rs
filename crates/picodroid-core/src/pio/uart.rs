// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub use super::fields::uart as fields;
use super::helpers::{extract_obj_idx, read_field};

use crate::hal::uart as platform;

fn extract_uart_id(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
    let idx = extract_obj_idx(args)?;
    match objects.get_field(idx, fields::UART_ID) {
        Some(Value::Int(id)) => Ok(id as u8),
        _ => Err(JvmError::InvalidReference),
    }
}

fn get_config(objects: &ObjectHeap, idx: u16) -> (i32, i32, i32, i32, i32) {
    (
        read_field(objects, idx, fields::BAUDRATE, 9600),
        read_field(objects, idx, fields::DATA_SIZE, 8),
        read_field(objects, idx, fields::PARITY, 0),
        read_field(objects, idx, fields::STOP_BITS, 1),
        read_field(objects, idx, fields::HW_FLOW, 0),
    )
}

/// Configure GPIO pins for UART function and start the UART with defaults (9600 8N1).
/// Called once from `peripheral_manager::open_uart()`.
pub fn init(uart_id: u8) {
    platform::init(uart_id);
}

pub fn set_baudrate_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let rate = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::BAUDRATE, Value::Int(rate))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, fields::UART_ID, 0) as u8;
    let (_, data_size, parity, stop_bits, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, rate, data_size, parity, stop_bits, hw_flow);
    Ok(None)
}

pub fn set_data_size_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let size = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::DATA_SIZE, Value::Int(size))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, fields::UART_ID, 0) as u8;
    let (baudrate, _, parity, stop_bits, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, size, parity, stop_bits, hw_flow);
    Ok(None)
}

pub fn set_parity_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let mode = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::PARITY, Value::Int(mode))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, fields::UART_ID, 0) as u8;
    let (baudrate, data_size, _, stop_bits, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, data_size, mode, stop_bits, hw_flow);
    Ok(None)
}

pub fn set_stop_bits_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let bits = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::STOP_BITS, Value::Int(bits))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, fields::UART_ID, 0) as u8;
    let (baudrate, data_size, parity, _, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, data_size, parity, bits, hw_flow);
    Ok(None)
}

pub fn set_hw_flow_ctrl_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let mode = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::HW_FLOW, Value::Int(mode))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, fields::UART_ID, 0) as u8;
    let (baudrate, data_size, parity, stop_bits, _) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, data_size, parity, stop_bits, mode);
    Ok(None)
}

/// Blocking write of a single byte. Returns `Some(Int(1))` on success.
pub fn write_byte_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let uart_id = extract_uart_id(args, objects)?;
    let byte = match args.get(1) {
        Some(Value::Int(v)) => *v as u8,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::write_byte(uart_id, byte);
    Ok(Some(Value::Int(1)))
}

/// Non-blocking read of a single byte. Returns `Some(Int(-1))` if RX FIFO is empty.
pub fn read_byte_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let uart_id = extract_uart_id(args, objects)?;
    Ok(Some(Value::Int(platform::read_byte(uart_id))))
}
