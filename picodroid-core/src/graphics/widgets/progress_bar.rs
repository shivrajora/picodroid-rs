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

/// `ProgressBar.nativeCreateIndeterminate(int argb)` — `argb` is the
/// theme-derived tint for the moving arc, supplied from Java
/// (`Theme.colorPrimary`) so callers can rebrand the spinner without
/// touching this shim.
pub fn progress_bar_native_create_indeterminate(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let argb = match args.first() {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    Ok(Some(Value::Int(lvgl_progress_bar::create_indeterminate(
        argb,
    ))))
}

/// `ProgressBar.nativeSetProgress(int value)` — the public `setProgress`
/// wrapper caches the value Java-side (so `getProgress()` is immediate
/// while the lv_bar animates) and skips the call entirely when
/// indeterminate.
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

/// `ProgressBar.setTint(int argbColor)` — silently ignored on a determinate
/// bar (the `lv_bar` track has no arc to tint). Matches Android's
/// `setIndeterminateTintList` which only affects the indeterminate
/// drawable.
pub fn progress_bar_set_tint(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_progress_bar::set_tint(id, argb);
    Ok(None)
}
