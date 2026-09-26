// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.graphics.Canvas`, `Paint`'s text metrics and
//! the drawing half of `picodroid.view.View` (`View(Context)`, `nativeBeginDraw`,
//! `nativeEndDraw`).
//!
//! A `Canvas` names no widget of its own: `View.nativeBeginDraw` writes the
//! drawing view's handle and size into the canvas's fields for the length of
//! `onDraw` and `nativeEndDraw` zeroes them, so a draw call outside `onDraw`
//! finds handle 0 and does nothing. Every draw native reads its `Paint` by
//! field slot (`graphics/fields.rs::paint`); Java marshals nothing.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::fields;
use super::super::lvgl::widgets::canvas::{self as lvgl_canvas, PaintSpec};
use super::super::view::{extract_native_handle, extract_string_at};

fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

fn arg_float(args: &[Value], i: usize) -> Result<f32, JvmError> {
    match args.get(i) {
        Some(Value::Float(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

fn arg_object(args: &[Value], i: usize) -> Result<u16, JvmError> {
    match args.get(i) {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

fn int_field(objects: &ObjectHeap, obj: u16, slot: usize) -> i32 {
    match objects.get_field(obj, slot) {
        Some(Value::Int(v)) => v,
        _ => 0,
    }
}

fn float_field(objects: &ObjectHeap, obj: u16, slot: usize) -> f32 {
    match objects.get_field(obj, slot) {
        Some(Value::Float(v)) => v,
        _ => 0.0,
    }
}

/// The `Paint` at `args[i]`.
fn paint_at(args: &[Value], i: usize, objects: &ObjectHeap) -> Result<PaintSpec, JvmError> {
    use fields::paint as f;
    let p = arg_object(args, i)?;
    Ok(PaintSpec {
        color: int_field(objects, p, f::COLOR) as u32,
        stroke_width: float_field(objects, p, f::STROKE_WIDTH),
        style: int_field(objects, p, f::STYLE),
        cap: int_field(objects, p, f::STROKE_CAP),
        text_size: float_field(objects, p, f::TEXT_SIZE),
        text_align: int_field(objects, p, f::TEXT_ALIGN),
    })
}

/// The drawing view's handle held by the receiving `Canvas` (`args[0]`); 0
/// outside `onDraw`.
fn canvas_handle(args: &[Value], objects: &ObjectHeap) -> Result<i32, JvmError> {
    let canvas = arg_object(args, 0)?;
    Ok(int_field(objects, canvas, fields::canvas::NATIVE_HANDLE))
}

/// `View.nativeCreateView()`: static, no arguments.
pub fn view_native_create_view() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_canvas::create())))
}

/// `View.nativeBeginDraw(Canvas)`: point the canvas at this view for `onDraw`.
pub fn view_native_begin_draw(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    use fields::canvas as f;
    let id = extract_native_handle(args, objects)?;
    let canvas = arg_object(args, 1)?;
    let (w, h) = lvgl_canvas::begin(id);
    objects
        .set_field(canvas, f::NATIVE_HANDLE, Value::Int(id))
        .ok_or(JvmError::InvalidReference)?;
    objects
        .set_field(canvas, f::WIDTH, Value::Int(w))
        .ok_or(JvmError::InvalidReference)?;
    objects
        .set_field(canvas, f::HEIGHT, Value::Int(h))
        .ok_or(JvmError::InvalidReference)?;
    Ok(None)
}

/// `View.nativeEndDraw(Canvas)`: detach the canvas and repaint the view.
pub fn view_native_end_draw(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    use fields::canvas as f;
    let id = extract_native_handle(args, objects)?;
    let canvas = arg_object(args, 1)?;
    for slot in [f::NATIVE_HANDLE, f::WIDTH, f::HEIGHT] {
        objects
            .set_field(canvas, slot, Value::Int(0))
            .ok_or(JvmError::InvalidReference)?;
    }
    lvgl_canvas::end(id);
    Ok(None)
}

/// `Canvas.nativeDrawColor(int color)`
pub fn canvas_draw_color(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = canvas_handle(args, objects)?;
    if id != 0 {
        lvgl_canvas::fill(id, arg_int(args, 1)? as u32);
    }
    Ok(None)
}

/// `Canvas.nativeDrawRect(float l, float t, float r, float b, float radius, Paint)`: rects,
/// round rects and circles.
pub fn canvas_draw_rect(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = canvas_handle(args, objects)?;
    let paint = paint_at(args, 6, objects)?;
    if id == 0 {
        return Ok(None);
    }
    let op = lvgl_canvas::rect_op(
        arg_float(args, 1)?,
        arg_float(args, 2)?,
        arg_float(args, 3)?,
        arg_float(args, 4)?,
        arg_float(args, 5)?,
        &paint,
    );
    if let Some(op) = op {
        lvgl_canvas::rect(id, &op);
    }
    Ok(None)
}

/// `Canvas.nativeDrawLine(float x1, float y1, float x2, float y2, Paint)`
pub fn canvas_draw_line(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = canvas_handle(args, objects)?;
    let paint = paint_at(args, 5, objects)?;
    if id != 0 {
        lvgl_canvas::line(
            id,
            arg_float(args, 1)?,
            arg_float(args, 2)?,
            arg_float(args, 3)?,
            arg_float(args, 4)?,
            &paint,
        );
    }
    Ok(None)
}

/// `Canvas.nativeDrawArc(float l, float t, float r, float b, float start, float sweep, Paint)`
pub fn canvas_draw_arc(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = canvas_handle(args, objects)?;
    let paint = paint_at(args, 7, objects)?;
    if id == 0 {
        return Ok(None);
    }
    let op = lvgl_canvas::arc_op(
        arg_float(args, 1)?,
        arg_float(args, 2)?,
        arg_float(args, 3)?,
        arg_float(args, 4)?,
        arg_float(args, 5)?,
        arg_float(args, 6)?,
        &paint,
    );
    if let Some(op) = op {
        lvgl_canvas::arc(id, &op, paint.color);
    }
    Ok(None)
}

/// `Canvas.nativeDrawText(String text, float x, float y, Paint)`
pub fn canvas_draw_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = canvas_handle(args, objects)?;
    let paint = paint_at(args, 4, objects)?;
    if id == 0 {
        return Ok(None);
    }
    let text = extract_string_at(args, 1, strings)?;
    lvgl_canvas::text(id, text, arg_float(args, 2)?, arg_float(args, 3)?, &paint);
    Ok(None)
}

/// The receiving `Paint`'s (`args[0]`) text size.
fn own_text_size(args: &[Value], objects: &ObjectHeap) -> Result<f32, JvmError> {
    let p = arg_object(args, 0)?;
    Ok(float_field(objects, p, fields::paint::TEXT_SIZE))
}

/// `Paint.ascent()`
pub fn paint_ascent(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let size = own_text_size(args, objects)?;
    Ok(Some(Value::Float(lvgl_canvas::ascent(size) as f32)))
}

/// `Paint.descent()`
pub fn paint_descent(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let size = own_text_size(args, objects)?;
    Ok(Some(Value::Float(lvgl_canvas::descent(size) as f32)))
}

/// `Paint.measureText(String)`
pub fn paint_measure_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let size = own_text_size(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    Ok(Some(Value::Float(
        lvgl_canvas::text_width(text, size) as f32
    )))
}
