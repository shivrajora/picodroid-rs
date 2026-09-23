// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ProgressBar`.
//!
//! The Java class caches progress, range and tints and tests its
//! `indeterminate` flag before every call, so the bar natives here only ever
//! see an `lv_bar` handle and the arc tint only an `lv_spinner` one.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::progress_bar as lvgl_progress_bar;
use super::super::view::extract_native_handle;

fn int_at(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `ProgressBar.nativeCreate()`
pub fn progress_bar_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_progress_bar::create())))
}

/// `ProgressBar.nativeCreateIndeterminate(int argb)` — `argb` is the
/// theme-derived tint for the moving arc, supplied from Java
/// (`Theme.colorPrimary`) so callers can rebrand the spinner without
/// touching this shim.
pub fn progress_bar_native_create_indeterminate(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let argb = int_at(args, 0)?;
    Ok(Some(Value::Int(lvgl_progress_bar::create_indeterminate(
        argb,
    ))))
}

/// `ProgressBar.nativeSetProgress(int value, boolean animate)` — the value
/// is already clamped to the range by Java, which also caches it so
/// `getProgress()` answers at once while the fill animates.
pub fn progress_bar_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let value = int_at(args, 1)?;
    let animate = int_at(args, 2)? != 0;
    lvgl_progress_bar::set_progress(id, value, animate);
    Ok(None)
}

/// `ProgressBar.nativeSetRange(int min, int max, int progress)` — `progress`
/// is the value Java holds after clamping to the new range.
pub fn progress_bar_set_range(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let min = int_at(args, 1)?;
    let max = int_at(args, 2)?;
    let progress = int_at(args, 3)?;
    lvgl_progress_bar::set_range(id, min, max, progress);
    Ok(None)
}

/// `ProgressBar.nativeSetTint(int target, int argb, boolean apply)`
pub fn progress_bar_set_tint(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let target = int_at(args, 1)?;
    let argb = int_at(args, 2)?;
    let apply = int_at(args, 3)? != 0;
    lvgl_progress_bar::set_tint(id, target, argb, apply);
    Ok(None)
}
