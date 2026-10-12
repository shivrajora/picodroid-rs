// SPDX-License-Identifier: GPL-3.0-only
use alloc::format;
use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use crate::shrink_names::c;

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

/// Refuse a pad the board's own wiring owns (`board_cfg::reserved`): an
/// `IllegalArgumentException` naming the pin and its role, so an app that
/// drives optional hardware on a pack's spare pads can catch it and carry on
/// without. Letting the open through would hand the display's DC line, say,
/// to the PWM block and leave the panel showing noise.
pub fn refuse_reserved_pin(
    pin: u8,
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<(), JvmError> {
    match crate::board_cfg::reserved::pin_role(pin) {
        None => Ok(()),
        Some(role) => {
            crate::pd_warn!("pio: GP{} refused, it is {} on this board", pin, role);
            Err(throw_illegal_argument(
                objects,
                strings,
                &format!("GP{pin} is {role} on this board"),
            ))
        }
    }
}

/// Refuse the display's SPI bus: its clock is the panel's, and a second
/// master on it would reprogram the rate under the frame writes.
pub fn refuse_display_spi_bus(
    spi_id: u8,
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<(), JvmError> {
    if !crate::board_cfg::reserved::spi_bus_is_display(spi_id) {
        return Ok(());
    }
    crate::pd_warn!(
        "pio: SPI{} refused, it is the display's bus on this board",
        spi_id
    );
    Err(throw_illegal_argument(
        objects,
        strings,
        &format!("SPI{spi_id} is the display's bus on this board"),
    ))
}

/// Allocate a `java/lang/IllegalArgumentException` carrying `msg`, by name
/// (the builtin hierarchy makes it catchable as a `RuntimeException`).
fn throw_illegal_argument(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    msg: &str,
) -> JvmError {
    match objects.alloc(c::java_lang_IllegalArgumentException) {
        Some(idx) => {
            if let Some(midx) = strings.intern_dyn(msg.as_bytes()) {
                let _ = objects.register_exception_message(idx, midx);
            }
            JvmError::Exception(idx)
        }
        None => JvmError::StackOverflow,
    }
}
