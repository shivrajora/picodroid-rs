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
    let Some(Value::Int(n)) = args.first() else {
        return Err(JvmError::InvalidReference);
    };
    let Some(ms) = sleep_millis(*n) else {
        return Ok(None);
    };
    if crate::host::stop_requested() {
        return Ok(None);
    }
    platform::sleep(ms);
    Ok(None)
}

/// The platform sleep to issue for `SystemClock.sleep(ms)`, or `None` for a
/// sleep that must return immediately. Android returns at once for a zero or
/// negative argument; a plain `as u32` turned `sleep(-1)` into 49.7 days.
fn sleep_millis(ms: i32) -> Option<u32> {
    u32::try_from(ms).ok().filter(|&ms| ms > 0)
}

#[cfg(test)]
mod tests {
    use super::sleep_millis;

    #[test]
    fn negative_and_zero_sleeps_return_immediately() {
        assert_eq!(sleep_millis(-1), None);
        assert_eq!(sleep_millis(i32::MIN), None);
        assert_eq!(sleep_millis(0), None);
        assert_eq!(sleep_millis(5), Some(5));
        assert_eq!(sleep_millis(i32::MAX), Some(i32::MAX as u32));
    }
}

pub fn elapsed_realtime_nanos() -> Result<Option<Value>, JvmError> {
    let nanos = platform::elapsed_realtime_nanos();
    Ok(Some(Value::Long(nanos)))
}

// ── Wall clock ───────────────────────────────────────────────────────────
//
// `System.currentTimeMillis()` counts from boot until something calls
// `SystemClock.setCurrentTimeMillis(epochMs)` (e.g. an SNTP sync); the
// anchor stored here is `epoch_ms − elapsed_ms` at set time, so reads stay
// driven by the monotonic clock and only the origin shifts.
//
// The 64-bit offset is kept in a store/load-only seqlock (two AtomicU32
// halves + a version word): thumbv6m/thumbv8m have no 64-bit atomics, and
// thumbv6m has no CAS at all. Single-writer by design — the one network
// thread syncs the clock; concurrent setters would be a last-write-wins
// race on the halves, which readers survive (they retry while the seq is
// odd or moving) but which could mix two writers' halves. Don't add a
// second caller without adding a mutex.

use core::sync::atomic::{AtomicU32, Ordering};

static WALL_SEQ: AtomicU32 = AtomicU32::new(0);
static WALL_HI: AtomicU32 = AtomicU32::new(0);
static WALL_LO: AtomicU32 = AtomicU32::new(0);

/// Current wall-clock offset in ms (0 until a sync has happened).
pub fn wall_offset_ms() -> i64 {
    loop {
        let s1 = WALL_SEQ.load(Ordering::Acquire);
        if s1 & 1 != 0 {
            continue; // write in progress
        }
        let hi = WALL_HI.load(Ordering::Acquire);
        let lo = WALL_LO.load(Ordering::Acquire);
        if WALL_SEQ.load(Ordering::Acquire) == s1 {
            return (((hi as u64) << 32) | lo as u64) as i64;
        }
    }
}

/// `SystemClock.setCurrentTimeMillis(long)` — anchors the wall clock.
/// Returns `true` (Android's return signals permission, which doesn't
/// apply here).
pub fn set_current_time_millis(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let millis = match args.first() {
        Some(Value::Long(n)) => *n,
        _ => return Err(JvmError::InvalidReference),
    };
    let elapsed_ms = platform::elapsed_realtime_nanos() / 1_000_000;
    let offset = (millis - elapsed_ms) as u64;
    let seq = WALL_SEQ.load(Ordering::Relaxed);
    WALL_SEQ.store(seq.wrapping_add(1), Ordering::Release); // odd: write begins
    WALL_HI.store((offset >> 32) as u32, Ordering::Release);
    WALL_LO.store(offset as u32, Ordering::Release);
    WALL_SEQ.store(seq.wrapping_add(2), Ordering::Release); // even: write done
    Ok(Some(Value::Int(1)))
}
