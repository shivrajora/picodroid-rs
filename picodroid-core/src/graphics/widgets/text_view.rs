// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.TextView`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::text_view as lvgl_text_view;
use super::super::view::{extract_native_handle, extract_string_at};

/// `TextView.nativeCreate()`
pub fn text_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_text_view::create())))
}

/// `TextView.setText(String text)`
pub fn text_view_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let s = extract_string_at(args, 1, strings)?;
    lvgl_text_view::set_text(id, s);
    Ok(None)
}

/// `TextView.getText()` — the label text as a fresh dyn string; an empty
/// string when LVGL has none, never `null` (Android returns "" too).
pub fn text_view_get_text(
    args: &[Value],
    strings: &mut StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let mut buf = [0u8; 256];
    let len = lvgl_text_view::get_text(id, &mut buf).unwrap_or(0);
    intern_text(&buf[..len], strings)
}

pub(super) fn intern_text(
    bytes: &[u8],
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let ref_idx = strings.intern_dyn(bytes).ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(ref_idx)))
}

/// `TextView.setTextColor(int argb)`
pub fn text_view_set_text_color(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_text_view::set_text_color(id, argb);
    Ok(None)
}

/// `TextView.setIncludeFontPadding(boolean include)`
pub fn text_view_set_include_font_padding(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let include = matches!(args.get(1), Some(Value::Int(v)) if *v != 0);
    lvgl_text_view::set_include_font_padding(id, include);
    Ok(None)
}
