// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    array_heap::ArrayHeap,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub use super::fields::i2c as fields;
use super::helpers::extract_obj_idx;

use crate::hal::i2c as platform;

fn extract_i2c_id(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
    let idx = extract_obj_idx(args)?;
    match objects.get_field(idx, fields::I2C_ID) {
        Some(Value::Int(id)) => Ok(id as u8),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Configure GPIO pins for I2C function and start the controller at 100 kHz.
/// Called once from `peripheral_manager::open_i2c()`.
pub fn init(i2c_id: u8) {
    platform::init(i2c_id);
}

pub fn set_speed_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let hz = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::SPEED_HZ, Value::Int(hz as i32))
        .ok_or(JvmError::StackOverflow)?;
    let id = match objects.get_field(idx, fields::I2C_ID) {
        Some(Value::Int(v)) => v as u8,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::set_speed(id, hz);
    Ok(None)
}

/// Blocking write. args: [this, Int(address), ArrayRef(data), Int(len)]
/// Returns Int(len) on success, Int(-1) on NACK/abort.
pub fn write_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let i2c_id = extract_i2c_id(args, objects)?;
    let address = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let data_idx = match args.get(2) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(3) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let result = platform::write(i2c_id, address, data_idx, len, arrays);
    Ok(Some(Value::Int(result)))
}

/// Blocking read. args: [this, Int(address), ArrayRef(buf), Int(len)]
/// Returns Int(len) on success, Int(-1) on NACK/abort.
pub fn read_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let i2c_id = extract_i2c_id(args, objects)?;
    let address = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let buf_idx = match args.get(2) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(3) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    if len == 0 {
        return Ok(Some(Value::Int(0)));
    }
    let result = platform::read(i2c_id, address, buf_idx, len, arrays);
    Ok(Some(Value::Int(result)))
}
