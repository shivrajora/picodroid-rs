// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.TimePicker`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::time_picker as lvgl_time_picker;
use super::super::view::extract_native_handle;

pub use lvgl_time_picker::reset_time_picker_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_time_picker::{drain_time_picker_queue, lookup_time_picker_obj};

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

pub fn time_picker_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_time_picker::create())))
}

pub fn time_picker_set_time(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let hour = arg_int(args, 1)?;
    let minute = arg_int(args, 2)?;
    lvgl_time_picker::set_time(id, hour, minute);
    Ok(None)
}

pub fn time_picker_get_hour(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(lvgl_time_picker::get_hour(id))))
}

pub fn time_picker_get_minute(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(lvgl_time_picker::get_minute(id))))
}

pub fn time_picker_register_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_time_picker::register_listener(id, obj_ref);
    Ok(None)
}

pub fn time_picker_set_is_24hour(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let on = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_time_picker::set_is_24hour(id, on);
    Ok(None)
}

pub fn time_picker_is_24hour(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let v = if lvgl_time_picker::is_24hour(id) {
        1
    } else {
        0
    };
    Ok(Some(Value::Int(v)))
}
