use pico_jvm::types::{JvmError, Value};

use crate::hal::system_clock as platform;

pub fn sleep(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let ms = match args.first() {
        Some(Value::Int(n)) => *n as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::sleep(ms);
    Ok(None)
}

pub fn elapsed_realtime_nanos() -> Result<Option<Value>, JvmError> {
    let nanos = platform::elapsed_realtime_nanos();
    Ok(Some(Value::Long(nanos)))
}
