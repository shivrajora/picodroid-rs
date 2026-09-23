// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.CircularProgressIndicator`.
//!
//! The class extends `ProgressBar`, and `ProgressBar.setProgress` reaches its
//! package-private `nativeSetProgress` through `invokevirtual`, which the JVM
//! dispatches by the receiver's runtime class — so a ring's progress arrives
//! here, not at the bar's shim, and the `lv_bar` setters never see an `lv_arc`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::circular_progress_indicator as lvgl_ring;
use super::super::view::extract_native_handle;

fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `CircularProgressIndicator.nativeCreate(int indicatorArgb, int trackArgb)` —
/// static, so the colours are `args[0]` and `args[1]`.
pub fn circular_progress_indicator_native_create(
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let indicator = arg_int(args, 0)?;
    let track = arg_int(args, 1)?;
    Ok(Some(Value::Int(lvgl_ring::create(indicator, track))))
}

/// `ProgressBar.nativeSetProgress(int value, boolean animate)` on a ring
/// receiver; the arc has no animated setter, so `animate` is ignored.
pub fn circular_progress_indicator_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_value(id, arg_int(args, 1)?);
    Ok(None)
}

/// `ProgressBar.nativeSetRange(int min, int max, int progress)` on a ring
/// receiver; `progress` is the value Java holds after clamping to the range.
pub fn circular_progress_indicator_set_range(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_range(id, arg_int(args, 1)?, arg_int(args, 2)?);
    lvgl_ring::set_value(id, arg_int(args, 3)?);
    Ok(None)
}

/// `CircularProgressIndicator.nativeSetIndicatorColor(int argb)`
pub fn circular_progress_indicator_set_indicator_color(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_indicator_color(id, arg_int(args, 1)?);
    Ok(None)
}

/// `CircularProgressIndicator.nativeSetTrackColor(int argb)`
pub fn circular_progress_indicator_set_track_color(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_track_color(id, arg_int(args, 1)?);
    Ok(None)
}

/// `CircularProgressIndicator.nativeSetTrackThickness(int px)`
pub fn circular_progress_indicator_set_track_thickness(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_track_thickness(id, arg_int(args, 1)?);
    Ok(None)
}

/// `CircularProgressIndicator.nativeSetIndicatorDirection(int direction)`
pub fn circular_progress_indicator_set_indicator_direction(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_indicator_direction(id, arg_int(args, 1)?);
    Ok(None)
}

/// `CircularProgressIndicator.nativeSetTrackCornerRadius(int radius)`
pub fn circular_progress_indicator_set_track_corner_radius(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_track_corner_radius(id, arg_int(args, 1)?);
    Ok(None)
}

/// `CircularProgressIndicator.nativeSetAngles(int startDegrees, int sweepDegrees)`
pub fn circular_progress_indicator_set_angles(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_ring::set_angles(id, arg_int(args, 1)?, arg_int(args, 2)?);
    Ok(None)
}
