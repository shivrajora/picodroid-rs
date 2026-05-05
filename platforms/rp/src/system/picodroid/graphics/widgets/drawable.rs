// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.graphics.drawable.GradientDrawable`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::drawable as lvgl_drawable;
use super::super::view::extract_handle_at;

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `GradientDrawable.nativeApply(View target, int fillColor, int radius,
///   int strokeWidth, int strokeColor, int hasGradient, int gradStart,
///   int gradEnd, int gradOrientation)`
///
/// Single bulk-apply rather than nine separate setters: every property
/// is written in one frame, no half-styled intermediate state visible to
/// the renderer. `target` is a `View` ObjectRef (taken instead of the
/// raw int handle so GradientDrawable stays out of `picodroid.view`,
/// where `nativeHandle` is package-private).
pub fn gradient_drawable_apply(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_handle_at(args, 0, objects)?;
    let fill_color = arg_int(args, 1)? as u32;
    let radius = arg_int(args, 2)?;
    let stroke_width = arg_int(args, 3)?;
    let stroke_color = arg_int(args, 4)? as u32;
    let has_gradient = arg_int(args, 5)? != 0;
    let grad_start = arg_int(args, 6)? as u32;
    let grad_end = arg_int(args, 7)? as u32;
    let grad_dir = arg_int(args, 8)? as u32;
    lvgl_drawable::apply_gradient_drawable(
        handle,
        fill_color,
        radius,
        stroke_width,
        stroke_color,
        has_gradient,
        grad_start,
        grad_end,
        grad_dir,
    );
    Ok(None)
}
