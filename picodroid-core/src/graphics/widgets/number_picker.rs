// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.NumberPicker`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::number_picker as lvgl_number_picker;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_number_picker::reset_number_picker_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_number_picker::{drain_step_queue as drain_np_step_queue, lookup_picker_obj};

/// `NumberPicker.nativeCreate()`
pub fn number_picker_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_number_picker::create())))
}

/// `NumberPicker.nativeSetText(String text)`
pub fn number_picker_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let s = extract_string_at(args, 1, strings)?;
    lvgl_number_picker::set_text(id, s);
    Ok(None)
}

/// `NumberPicker.nativeRegisterPicker()`
pub fn number_picker_register_picker(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_number_picker::register_picker(id, obj_ref);
    Ok(None)
}
