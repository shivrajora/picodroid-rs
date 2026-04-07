use crate::lvgl_ffi::*;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::extract_native_handle;

/// `ProgressBar.nativeCreate()` — creates an `lv_bar`.
pub fn progress_bar_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe {
        let b = lv_bar_create(engine::screen());
        lv_bar_set_value(b, 0, LV_ANIM_OFF);
        b
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `ProgressBar.setProgress(int value)`
pub fn progress_bar_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let value = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_bar_set_value(handle_table::lookup(id), value, LV_ANIM_ON) };
    Ok(None)
}
