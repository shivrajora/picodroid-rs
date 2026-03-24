use pico_jvm::types::{JvmError, Value};

#[cfg(not(feature = "sim"))]
#[path = "system_clock/real.rs"]
mod platform;
#[cfg(feature = "sim")]
#[path = "system_clock/sim.rs"]
mod platform;

pub fn sleep(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let ms = match args.first() {
        Some(Value::Int(n)) => *n as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::sleep(ms);
    Ok(None)
}
