use crate::framework::{
    heap::StringTable,
    types::{JvmError, Value},
};

/// Native implementation of `picodroid.util.Log.i(String tag, String msg)`.
/// Resolves both string references and emits a defmt log line.
pub fn log_i(args: &[Value], strings: &StringTable) -> Result<(), JvmError> {
    let tag = resolve(args.get(0).copied().unwrap_or(Value::Null), strings)?;
    let msg = resolve(args.get(1).copied().unwrap_or(Value::Null), strings)?;
    defmt::info!("{=str}: {=str}", tag, msg);
    Ok(())
}

fn resolve(v: Value, strings: &StringTable) -> Result<&'static str, JvmError> {
    match v {
        Value::Reference(idx) => strings.resolve(idx).ok_or(JvmError::InvalidReference),
        _ => Err(JvmError::InvalidReference),
    }
}
