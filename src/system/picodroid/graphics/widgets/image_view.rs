//! Java-binding shim for `picodroid.widget.ImageView`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::image_view as lvgl_image_view;
use super::super::view::extract_native_handle;

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

/// `ImageView.setScaleType(int scaleType)`
pub fn image_view_set_scale_type(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let scale_type = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_image_view::set_scale_type(id, scale_type);
    Ok(None)
}

/// `ImageView.setTint(int argbColor)`
pub fn image_view_set_tint(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_image_view::set_tint(id, argb);
    Ok(None)
}

/// `ImageView.setScale(int zoom)`
pub fn image_view_set_scale(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let zoom = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_image_view::set_scale(id, zoom);
    Ok(None)
}
