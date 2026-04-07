use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;

/// `ImageView.nativeCreate()` — creates an `lv_image`.
pub fn image_view_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe { lv_image_create(engine::screen()) };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `ImageView.setImageSource(String path)` — stub (no filesystem on embedded).
pub fn image_view_set_src(
    _args: &[Value],
    _strings: &StringTable,
    _objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    // Image loading from paths is not supported on embedded targets.
    // This is a placeholder for future built-in image descriptor support.
    Ok(None)
}
