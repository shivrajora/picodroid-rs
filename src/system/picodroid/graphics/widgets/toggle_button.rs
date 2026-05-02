// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ToggleButton`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::toggle_button as lvgl_toggle_button;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_toggle_button::reset_toggle_button_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_toggle_button::{drain_checked_change_queue, lookup_checked_change_obj};

pub fn toggle_button_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_toggle_button::create())))
}

pub fn toggle_button_native_create_with_text(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let text_on = extract_string_at(args, 0, strings)?;
    let text_off = extract_string_at(args, 1, strings)?;
    Ok(Some(Value::Int(lvgl_toggle_button::create_with_text(
        text_on, text_off,
    ))))
}

pub fn toggle_button_is_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(if lvgl_toggle_button::is_checked(id) {
        1
    } else {
        0
    })))
}

pub fn toggle_button_set_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_toggle_button::set_checked(id, checked);
    Ok(None)
}

pub fn toggle_button_toggle(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_toggle_button::toggle(id);
    Ok(None)
}

pub fn toggle_button_perform_checked_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_toggle_button::perform_checked_change(id);
    Ok(None)
}

pub fn toggle_button_set_text_on(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_toggle_button::set_text_on(id, text);
    Ok(None)
}

pub fn toggle_button_set_text_off(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_toggle_button::set_text_off(id, text);
    Ok(None)
}

pub fn toggle_button_register_checked_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_toggle_button::register_listener(id, obj_ref);
    Ok(None)
}
