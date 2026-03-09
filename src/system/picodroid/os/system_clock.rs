use crate::framework::types::{JvmError, Value};

pub fn sleep(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let ms = match args.get(0) {
        Some(Value::Int(n)) => *n as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(ms));
    Ok(None)
}
