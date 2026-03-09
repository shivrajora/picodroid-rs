use crate::framework::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub fn get_instance(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    let idx = objects
        .alloc("picodroid/pio/PeripheralManager")
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::ObjectRef(idx)))
}

pub fn open_gpio(
    args: &[Value],
    strings: &StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    // args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "GPxx" string
    let name_ref = match args.get(1) {
        Some(Value::Reference(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let name = strings
        .resolve(name_ref)
        .ok_or(JvmError::InvalidReference)?;

    // Parse "GPxx" → pin number
    let pin_str = name.strip_prefix("GP").ok_or(JvmError::InvalidReference)?;
    let mut pin: u8 = 0;
    for c in pin_str.chars() {
        let d = (c as u8).wrapping_sub(b'0');
        if d > 9 {
            return Err(JvmError::InvalidReference);
        }
        pin = pin.wrapping_mul(10).wrapping_add(d);
    }

    let obj_idx = objects
        .alloc("picodroid/pio/Gpio")
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, 0, Value::Int(pin as i32))
        .ok_or(JvmError::StackOverflow)?;

    Ok(Some(Value::ObjectRef(obj_idx)))
}
