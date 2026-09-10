// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.view.ViewPropertyAnimator`.
//!
//! All three natives are *static* (the Java side passes the View's
//! nativeHandle as an explicit `int`), so none reads `this`.

use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::animations;

pub use animations::drain_completed_end_action;
pub use animations::reset_animation_state;

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

#[inline]
fn arg_float(args: &[Value], i: usize) -> Result<f32, JvmError> {
    match args.get(i) {
        Some(Value::Float(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `ViewPropertyAnimator.nativeStart(int handle, int property, float to, int durationMs,
/// int startDelayMs, int interpolator)` — to-only; the engine reads the implicit `from`.
pub fn animator_native_start(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = arg_int(args, 0)?;
    let property = arg_int(args, 1)?;
    let to = arg_float(args, 2)?;
    let duration_ms = arg_int(args, 3)?.max(0) as u32;
    let delay_ms = arg_int(args, 4)?.max(0) as u32;
    let interpolator = arg_int(args, 5)?;
    animations::start_to(handle, property, to, duration_ms, delay_ms, interpolator);
    Ok(None)
}

/// `ViewPropertyAnimator.nativeSetEndAction(int handle, Runnable action)`
pub fn animator_native_set_end_action(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = arg_int(args, 0)?;
    let obj_ref = match args.get(1) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    animations::set_end_action(handle, obj_ref);
    Ok(None)
}

/// `ViewPropertyAnimator.nativeCancel(int handle)` — cancels every animation targeting `handle`.
pub fn animator_native_cancel(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = arg_int(args, 0)?;
    animations::cancel(handle);
    Ok(None)
}
