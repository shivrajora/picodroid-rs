use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

#[cfg(not(feature = "sim"))]
#[path = "gpio/real.rs"]
mod platform;
#[cfg(feature = "sim")]
#[path = "gpio/sim.rs"]
mod platform;

pub fn set_direction_native(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let pin = extract_pin(args, objects)?;
    let direction = match args.get(1) {
        Some(Value::Int(d)) => *d,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::set_direction(pin, direction);
    Ok(None)
}

pub fn set_value_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let pin = extract_pin(args, objects)?;
    let high = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::set_value(pin, high);
    Ok(None)
}

fn extract_pin(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => match objects.get_field(*idx, 0) {
            Some(Value::Int(pin)) => Ok(pin as u8),
            _ => Err(JvmError::InvalidReference),
        },
        _ => Err(JvmError::InvalidReference),
    }
}
