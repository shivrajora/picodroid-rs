// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.LinearLayout`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::gfx::Handle;
use super::super::lvgl::widgets::linear_layout as lvgl_linear_layout;
use super::super::lvgl::with_gfx;
use super::super::view::{extract_handle_at, extract_native_handle};

pub fn linear_layout_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_linear_layout::create())))
}

/// `LinearLayout.addView(View child)`
pub fn linear_layout_add_view(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let parent_id = extract_native_handle(args, objects)?;
    let child_id = extract_handle_at(args, 1, objects)?;
    with_gfx(|g| g.set_parent(Handle::from_java(child_id), Handle::from_java(parent_id)));
    Ok(None)
}

pub fn linear_layout_set_orientation(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let orientation = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_linear_layout::set_orientation(id, orientation);
    Ok(None)
}

pub fn linear_layout_set_spacing(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let spacing = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_linear_layout::set_spacing(id, spacing);
    Ok(None)
}
