// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.RadioButton`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::radio_button as lvgl_radio_button;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_radio_button::reset_radio_button_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_radio_button::{drain_rb_checked_change_queue, lookup_rb_checked_change_obj};

pub fn radio_button_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_radio_button::create())))
}

pub fn radio_button_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_radio_button::set_text(id, text);
    Ok(None)
}

pub fn radio_button_is_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(if lvgl_radio_button::is_checked(id) {
        1
    } else {
        0
    })))
}

pub fn radio_button_set_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_radio_button::set_checked(id, checked);
    Ok(None)
}

pub fn radio_button_perform_checked_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_radio_button::perform_checked_change(id);
    Ok(None)
}

pub fn radio_button_register_checked_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_radio_button::register_listener(id, obj_ref);
    Ok(None)
}
