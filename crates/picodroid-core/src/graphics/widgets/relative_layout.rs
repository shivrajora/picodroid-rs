// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.RelativeLayout`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::relative_layout as lvgl_relative_layout;
use super::super::view::{arg_int, extract_native_handle};

pub fn relative_layout_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_relative_layout::create())))
}

/// `RelativeLayout.setGravity(int gravity)`
pub fn relative_layout_set_gravity(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_relative_layout::set_gravity(id, arg_int(args, 1)?);
    Ok(None)
}
