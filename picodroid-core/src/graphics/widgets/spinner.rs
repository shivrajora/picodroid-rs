// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.Spinner`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::spinner as lvgl_spinner;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_spinner::reset_spinner_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_spinner::{drain_spinner_change_queue, lookup_spinner_obj};

pub fn spinner_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_spinner::create())))
}

pub fn spinner_set_items(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let items = extract_string_at(args, 1, strings)?;
    lvgl_spinner::set_items(id, items);
    Ok(None)
}

pub fn spinner_get_selected(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(lvgl_spinner::get_selected(id))))
}

pub fn spinner_perform_item_selected(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_spinner::perform_item_selected(id);
    Ok(None)
}

pub fn spinner_register_item_selected_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_spinner::register_listener(id, obj_ref);
    Ok(None)
}
