// SPDX-License-Identifier: GPL-3.0-only
//! `Value`-level glue between `picodroid.media.ToneGenerator` and [`super::driver`].
//!
//! Routing lives in `native_handler/media.rs`; this is the argument unpacking,
//! kept beside the code it drives the way `pio/pwm.rs` sits beside
//! `native_handler/pio.rs`.

use pico_jvm::{
    array_heap::ArrayHeap,
    types::{JvmError, Value},
};

use super::driver;
use super::sequencer::MAX_SEGMENTS;

/// The JVM hands a `boolean` back as an int.
fn boolean(v: bool) -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(v as i32)))
}

fn int_arg(args: &[Value], at: usize) -> Result<i32, JvmError> {
    match args.get(at) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// args: `[this, Int(streamType), Int(volume)]`
///
/// `streamType` is read and dropped. There is no mixer to select with it, and
/// the Java side documents it as accepted for source compatibility only.
pub fn native_init(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let _stream_type = int_arg(args, 1)?;
    let volume = int_arg(args, 2)?;
    driver::init(volume);
    Ok(None)
}

/// args: `[this, Int(toneType), Int(durationMs)]` -> `Z`
pub fn start_tone(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let tone_type = int_arg(args, 1)?;
    let duration_ms = int_arg(args, 2)?;
    boolean(driver::start_tone(tone_type, duration_ms))
}

/// args: `[this, ArrayRef(freqHz), ArrayRef(durationMs)]` -> `Z`
///
/// A `null` array or a length mismatch is `false` rather than a thrown
/// exception: `startToneSequence` is documented as reporting refusal that way,
/// and a melody that will not play is not worth unwinding an app for.
pub fn start_tone_sequence(args: &[Value], arrays: &ArrayHeap) -> Result<Option<Value>, JvmError> {
    let (Some(Value::ArrayRef(freq_idx)), Some(Value::ArrayRef(dur_idx))) =
        (args.get(1), args.get(2))
    else {
        return boolean(false);
    };
    let (Some(freq_len), Some(dur_len)) = (arrays.length(*freq_idx), arrays.length(*dur_idx))
    else {
        return boolean(false);
    };
    let len = freq_len as usize;
    if freq_len != dur_len || len == 0 || len > MAX_SEGMENTS {
        return boolean(false);
    }

    // Copied out here rather than borrowed: the sequencer outlives this call
    // by minutes and the JVM heap moves under a collection.
    let mut freqs = [0i32; MAX_SEGMENTS];
    let mut durations = [0i32; MAX_SEGMENTS];
    for i in 0..len {
        let (Some(f), Some(d)) = (arrays.load(*freq_idx, i), arrays.load(*dur_idx, i)) else {
            return boolean(false);
        };
        freqs[i] = f;
        durations[i] = d;
    }
    boolean(driver::start_sequence(&freqs[..len], &durations[..len]))
}

/// args: `[this]`
pub fn stop_tone(_args: &[Value]) -> Result<Option<Value>, JvmError> {
    driver::stop();
    Ok(None)
}

/// args: `[this]`
pub fn release(_args: &[Value]) -> Result<Option<Value>, JvmError> {
    driver::release();
    Ok(None)
}
