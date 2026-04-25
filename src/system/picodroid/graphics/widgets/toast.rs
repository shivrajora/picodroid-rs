//! Java-binding shim for `picodroid.widget.Toast`.
//!
//! Toast is *not* a `View` subclass (no `nativeHandle` field), so instance
//! methods receive the handle as an explicit `int` argument rather than
//! reading it off `this`.

use pico_jvm::heap::StringTable;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::toast as lvgl_toast;
use super::super::view::extract_string_at;

pub use lvgl_toast::reset_toast_state;

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `Toast.nativeCreate(String text, int duration) -> int handle`
pub fn toast_native_create(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let text = extract_string_at(args, 0, strings).unwrap_or("");
    let duration = arg_int(args, 1)?;
    Ok(Some(Value::Int(lvgl_toast::create(text, duration))))
}

/// `Toast.nativeShow(int handle)`
pub fn toast_native_show(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    lvgl_toast::show(id);
    Ok(None)
}

/// `Toast.nativeCancel(int handle)`
pub fn toast_native_cancel(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    lvgl_toast::cancel(id);
    Ok(None)
}
