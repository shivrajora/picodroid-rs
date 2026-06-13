// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.Button`.
//!
//! Click-related plumbing (register/perform_click/was_clicked) lives on
//! `View` now since any view can be clickable; see [`super::view`].

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::button as lvgl_button;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_button::reset_button_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_button::{
    drain_click_queue, drain_long_click_queue, lookup_button_obj, lookup_long_click_obj,
};

/// `Button.nativeCreate(String text)`
pub fn button_native_create(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let text = extract_string_at(args, 0, strings).unwrap_or("");
    Ok(Some(Value::Int(lvgl_button::create(text))))
}

/// `Button.setText(String text)`
pub fn button_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_button::set_text(id, text);
    Ok(None)
}
