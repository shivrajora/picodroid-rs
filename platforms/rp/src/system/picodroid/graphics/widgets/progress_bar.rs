// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ProgressBar`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::progress_bar as lvgl_progress_bar;
use super::super::view::extract_native_handle;

pub use lvgl_progress_bar::reset_progress_bar_state;

/// `ProgressBar.nativeCreate()`
pub fn progress_bar_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_progress_bar::create())))
}

/// `ProgressBar.nativeCreateIndeterminate()`
pub fn progress_bar_native_create_indeterminate() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_progress_bar::create_indeterminate())))
}

/// `ProgressBar.setProgress(int value)`
pub fn progress_bar_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let value = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_progress_bar::set_progress(id, value);
    Ok(None)
}
