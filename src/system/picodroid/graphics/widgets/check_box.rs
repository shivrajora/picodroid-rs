//! Java-binding shim for `picodroid.widget.CheckBox`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::check_box as lvgl_check_box;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_check_box::reset_check_box_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_check_box::{drain_cb_checked_change_queue, lookup_cb_checked_change_obj};

pub fn check_box_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_check_box::create())))
}

pub fn check_box_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_check_box::set_text(id, text);
    Ok(None)
}

pub fn check_box_is_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(if lvgl_check_box::is_checked(id) {
        1
    } else {
        0
    })))
}

pub fn check_box_set_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_check_box::set_checked(id, checked);
    Ok(None)
}

pub fn check_box_perform_checked_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_check_box::perform_checked_change(id);
    Ok(None)
}

pub fn check_box_register_checked_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_check_box::register_listener(id, obj_ref);
    Ok(None)
}
