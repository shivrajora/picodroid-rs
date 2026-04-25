//! Java-binding shim for `picodroid.view.ViewPropertyAnimator`.
//!
//! Both natives are *static* (the Java side passes the View's
//! nativeHandle as an explicit `int`), so neither reads `this`.

use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::animations;

pub use animations::reset_animation_state;

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `ViewPropertyAnimator.nativeStart(int handle, int property, int from, int to, int durationMs)`
pub fn animator_native_start(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = arg_int(args, 0)?;
    let property = arg_int(args, 1)?;
    let from = arg_int(args, 2)?;
    let to = arg_int(args, 3)?;
    let duration_ms = arg_int(args, 4)?.max(0) as u32;
    animations::start(handle, property, from, to, duration_ms);
    Ok(None)
}

/// `ViewPropertyAnimator.nativeCancel(int handle)` — cancels every animation targeting `handle`.
pub fn animator_native_cancel(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = arg_int(args, 0)?;
    animations::cancel(handle);
    Ok(None)
}
