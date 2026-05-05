// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ListView`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::list_view as lvgl_list_view;
use super::super::view::{extract_native_handle, extract_string_at};

pub fn list_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_list_view::create())))
}

pub fn list_view_add_item(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_list_view::add_item(id, text);
    Ok(None)
}
