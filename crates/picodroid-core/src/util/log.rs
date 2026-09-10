// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    heap::StringTable,
    types::{JvmError, Value},
};

/// Severity of a `picodroid.util.Log` call. Maps onto defmt's level ladder on
/// hardware; the simulator prints every level in the same `[Tag] message`
/// stdout format so sim-test log-token greps stay level-agnostic.
#[derive(Copy, Clone)]
pub enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
}

/// Native implementation of `picodroid.util.Log.{v,d,i,w,e}(String tag, String msg)`.
/// Resolves both string references and emits a log line at `level`.
pub fn log(level: LogLevel, args: &[Value], strings: &StringTable) -> Result<(), JvmError> {
    let tag = resolve(args.first().copied().unwrap_or(Value::Null), strings)?;
    let msg = resolve(args.get(1).copied().unwrap_or(Value::Null), strings)?;
    #[cfg(not(feature = "sim"))]
    match level {
        LogLevel::Verbose => defmt::trace!("{=str}: {=str}", tag, msg),
        LogLevel::Debug => defmt::debug!("{=str}: {=str}", tag, msg),
        LogLevel::Info => defmt::info!("{=str}: {=str}", tag, msg),
        LogLevel::Warn => defmt::warn!("{=str}: {=str}", tag, msg),
        LogLevel::Error => defmt::error!("{=str}: {=str}", tag, msg),
    }
    #[cfg(feature = "sim")]
    {
        let _ = level;
        println!("[{}] {}", tag, msg);
    }
    Ok(())
}

fn resolve(v: Value, strings: &StringTable) -> Result<&str, JvmError> {
    match v {
        Value::Reference(idx) => strings.resolve(idx).ok_or(JvmError::InvalidReference),
        _ => Err(JvmError::InvalidReference),
    }
}
