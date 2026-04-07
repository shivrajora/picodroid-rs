use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_native_handle, java_str_to_cstr};

// ---------------------------------------------------------------------------
// Native method implementations
// ---------------------------------------------------------------------------

/// `EditText.nativeCreate()` -- creates an `lv_textarea`.
pub fn edit_text_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe { lv_textarea_create(engine::screen()) };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `EditText.setText(String text)`
pub fn edit_text_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(
        args.get(1).ok_or(JvmError::InvalidReference)?,
        strings,
        &mut buf,
    )?;
    unsafe { lv_textarea_set_text(handle_table::lookup(id), cstr) };
    Ok(None)
}

/// `EditText.getText()` -- returns the current textarea content as a Java String.
pub fn edit_text_get_text(
    args: &[Value],
    strings: &mut StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let cstr = unsafe { lv_textarea_get_text(handle_table::lookup(id)) };
    if cstr.is_null() {
        return Ok(Some(Value::Null));
    }
    // Measure the C string length (capped at 256).
    let mut len = 0usize;
    unsafe {
        while *cstr.add(len) != 0 && len < 256 {
            len += 1;
        }
    }
    // Build a byte slice from the raw c_char pointer.
    let mut buf = [0u8; 256];
    for (i, slot) in buf[..len].iter_mut().enumerate() {
        *slot = unsafe { *cstr.add(i) } as u8;
    }
    let bytes = &buf[..len];
    let ref_idx = strings.intern_dyn(bytes).ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(ref_idx)))
}

/// `EditText.setHint(String hint)`
pub fn edit_text_set_hint(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(
        args.get(1).ok_or(JvmError::InvalidReference)?,
        strings,
        &mut buf,
    )?;
    unsafe { lv_textarea_set_placeholder_text(handle_table::lookup(id), cstr) };
    Ok(None)
}
