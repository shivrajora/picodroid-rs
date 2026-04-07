use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_native_handle, java_str_to_cstr};

/// `ListView.nativeCreate()` — creates an `lv_list`.
pub fn list_view_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe { lv_list_create(engine::screen()) };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `ListView.addItem(String text)`
pub fn list_view_add_item(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe { lv_list_add_text(handle_table::lookup(id), cstr) };
    Ok(None)
}
