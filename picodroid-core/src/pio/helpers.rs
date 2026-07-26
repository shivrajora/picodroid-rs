// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

/// Extract the object index from the first argument (the `this` receiver).
/// Shared by uart, spi, i2c, pwm, adc, and gpio modules.
pub fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Read an integer field from an object, returning `default` if the field is absent or
/// not an `Int`.  Shared by uart and spi modules.
pub fn read_field(objects: &ObjectHeap, idx: u16, field: usize, default: i32) -> i32 {
    match objects.get_field(idx, field) {
        Some(Value::Int(v)) => v,
        _ => default,
    }
}

/// Extract the device name string from `args[1]` (a `Reference` into the string table).
/// Shared by all peripheral opener functions.
pub fn extract_device_name<'a>(
    args: &[Value],
    strings: &'a StringTable,
) -> Result<&'a str, JvmError> {
    let name_ref = match args.get(1) {
        Some(Value::Reference(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    strings.resolve(name_ref).ok_or(JvmError::InvalidReference)
}

/// Parse a bus peripheral name like "UART0", "SPI1", "I2C0" into a bus ID (0 or 1).
/// `prefix` is the expected prefix string (e.g. "UART", "SPI", "I2C").
/// Returns `Err(InvalidReference)` if the prefix is missing or the ID is not 0 or 1.
pub fn parse_bus_id(name: &str, prefix: &str) -> Result<u8, JvmError> {
    let id_str = name
        .strip_prefix(prefix)
        .ok_or(JvmError::InvalidReference)?;
    match id_str {
        "0" => Ok(0),
        "1" => Ok(1),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Allocate a peripheral object on the heap and store its ID in field slot 0.
/// Returns the heap index of the new object.
pub fn alloc_peripheral_with_id(
    objects: &mut ObjectHeap,
    class_name: &'static str,
    id_field: usize,
    id: u8,
) -> Result<u16, JvmError> {
    let obj_idx = objects.alloc(class_name).ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, id_field, Value::Int(id as i32))
        .ok_or(JvmError::StackOverflow)?;
    Ok(obj_idx)
}
