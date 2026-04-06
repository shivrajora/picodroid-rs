use crate::lvgl_ffi::*;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::view::extract_native_handle;

/// `Switch.nativeCreate()` — creates an `lv_switch`.
pub fn switch_native_create() -> Result<Option<Value>, JvmError> {
    let sw = unsafe { lv_switch_create(engine::screen()) };
    Ok(Some(Value::Int(sw as i32)))
}

/// `Switch.isChecked()`
pub fn switch_is_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let checked = unsafe { lv_obj_has_state(handle as *mut lv_obj_t, LV_STATE_CHECKED) };
    Ok(Some(Value::Int(if checked { 1 } else { 0 })))
}

/// `Switch.setChecked(boolean checked)`
pub fn switch_set_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = handle as *mut lv_obj_t;
        if checked {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        }
    }
    Ok(None)
}
