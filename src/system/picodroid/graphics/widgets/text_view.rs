use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::view::{extract_native_handle, java_str_to_cstr};

/// `TextView.nativeCreate()` — creates an `lv_label` on the active screen.
pub fn text_view_native_create() -> Result<Option<Value>, JvmError> {
    let handle = unsafe { lv_label_create(engine::screen()) };
    Ok(Some(Value::Int(handle as i32)))
}

/// `TextView.setText(String text)`
pub fn text_view_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe { lv_label_set_text(handle as *mut lv_obj_t, cstr) };
    Ok(None)
}

/// `TextView.setTextColor(int argb)`
pub fn text_view_set_text_color(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let color = lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    };
    unsafe { lv_obj_set_style_text_color(handle as *mut lv_obj_t, color, 0) };
    Ok(None)
}
