//! Java-binding shim for `picodroid.widget.SeekBar`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::seek_bar as lvgl_seek_bar;
use super::super::view::extract_native_handle;

pub use lvgl_seek_bar::reset_seek_bar_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_seek_bar::{drain_seek_change_queue, lookup_seek_bar_obj};

pub fn seek_bar_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_seek_bar::create())))
}

pub fn seek_bar_native_create_with_max(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let max = match args.first() {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    Ok(Some(Value::Int(lvgl_seek_bar::create_with_max(max))))
}

pub fn seek_bar_set_max(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let max = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_seek_bar::set_max(id, max);
    Ok(None)
}

pub fn seek_bar_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let progress = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_seek_bar::set_progress(id, progress);
    Ok(None)
}

pub fn seek_bar_get_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(lvgl_seek_bar::get_progress(id))))
}

pub fn seek_bar_perform_progress_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_seek_bar::perform_progress_change(id);
    Ok(None)
}

pub fn seek_bar_register_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_seek_bar::register_listener(id, obj_ref);
    Ok(None)
}
