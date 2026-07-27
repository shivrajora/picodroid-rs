// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::types::{JvmError, Value};

use crate::hal::system_clock as platform;

/// `SystemClock.sleep` — blocks the calling Java thread.
///
/// Returns immediately once a debug bridge has asked the JVM to stop, so a
/// `Thread.sleep(10_000)` cannot hold an install waiting ten seconds for a
/// safepoint.
///
/// The check lives here rather than in each family's `HalClock::sleep`. It
/// was the RP HAL's, which meant the simulator did not do it — a real
/// `Thread.sleep` interruptibility divergence — and meant every future family
/// had to know to repeat it, with nothing in the trait, the contract or the
/// porting guide saying so. Shared code asking `stop_requested()` is the same
/// question asked once.
pub fn sleep(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let ms = match args.first() {
        Some(Value::Int(n)) => *n as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    if crate::host::stop_requested() {
        return Ok(None);
    }
    platform::sleep(ms);
    Ok(None)
}

pub fn elapsed_realtime_nanos() -> Result<Option<Value>, JvmError> {
    let nanos = platform::elapsed_realtime_nanos();
    Ok(Some(Value::Long(nanos)))
}
