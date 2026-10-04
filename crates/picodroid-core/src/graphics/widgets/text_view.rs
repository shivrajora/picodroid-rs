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

/// `TextView.nativeSetText(String text)` — the label's text; a `Button`
/// receiver lands here too and reaches its child label (see `label_of`).
/// `TextView.getText()` is Java's own copy of what it last set.
pub fn text_view_native_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let s = extract_string_at(args, 1, strings)?;
    lvgl_text_view::set_text(id, s);
    Ok(None)
}

/// `TextView.nativeSetTextColor(int argb)`
pub fn text_view_native_set_text_color(
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

/// `TextView.nativeSetIncludeFontPadding(boolean include)`
pub fn text_view_native_set_include_font_padding(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let include = matches!(args.get(1), Some(Value::Int(v)) if *v != 0);
    lvgl_text_view::set_include_font_padding(id, include);
    Ok(None)
}

/// `TextView.nativeSetTextSize(float px)` — the size in pixels, snapped to a compiled face on the
/// LVGL side; a `Button` receiver lands here too (see `label_of`).
pub fn text_view_native_set_text_size(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let px = match args.get(1) {
        Some(Value::Float(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_text_view::set_text_size(id, px);
    Ok(None)
}

/// `TextView.nativeSetGravity(int gravity)` — the Android gravity bitmask; the LVGL side reads
/// its horizontal field as the label's text alignment.
pub fn text_view_native_set_gravity(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let gravity = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_text_view::set_gravity(id, gravity);
    Ok(None)
}

/// `TextView.nativeGetLineHeight()` — one line of the face in use, in pixels.
pub fn text_view_native_get_line_height(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(lvgl_text_view::line_height(id))))
}

/// `TextView.nativeSetLineMode(int ellipsize, int maxLines, boolean singleLine)` — the Java
/// side's packed line mode; a `Button` receiver lands here too (see `label_of`).
pub fn text_view_native_set_line_mode(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let int_at = |i: usize| match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    };
    let kind = int_at(1)?;
    let max_lines = int_at(2)?;
    let single = int_at(3)? != 0;
    lvgl_text_view::set_line_mode(id, kind, max_lines, single);
    Ok(None)
}
