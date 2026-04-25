//! Java-binding shim for `picodroid.widget.ImageView`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::image_view as lvgl_image_view;

pub fn image_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_image_view::create())))
}

/// `ImageView.setImageSource(String path)` — stub (no filesystem on embedded).
pub fn image_view_set_src(
    _args: &[Value],
    _strings: &StringTable,
    _objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    // Image loading from paths is not supported on embedded targets.
    // Placeholder for future built-in image descriptor support.
    Ok(None)
}
