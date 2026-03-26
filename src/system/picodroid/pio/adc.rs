use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub use super::fields::adc as fields;

#[cfg(not(feature = "sim"))]
#[path = "adc/real.rs"]
mod platform;
#[cfg(feature = "sim")]
#[path = "adc/sim.rs"]
mod platform;

fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Configure GPIO pin for ADC function. Called once from `peripheral_manager::open_adc()`.
pub fn init(pin: u8) {
    platform::init(pin);
}

/// Read the current ADC voltage. Returns a Value::Double with voltage in volts (0.0–3.3).
pub fn read_value_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let pin = match objects.get_field(idx, fields::PIN) {
        Some(Value::Int(p)) => p as u8,
        _ => return Err(JvmError::InvalidReference),
    };
    let voltage = platform::read(pin);
    Ok(Some(Value::Double(voltage)))
}
