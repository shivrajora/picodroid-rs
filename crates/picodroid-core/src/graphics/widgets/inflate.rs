// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `LayoutInflater.nativeInflateNode`: the one native
//! that makes a framework view from a layout node (`lvgl/inflate.rs`), and
//! writes what the Java side keeps — the wrapper's handle and cached
//! fields, the inflater's read position and `LayoutParams` words, and the
//! attributes handed back for Java's own setters.

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

use super::super::fields;
use super::super::inflate_scan as scan;
use super::super::lvgl::inflate as lvgl_inflate;
use crate::shrink_names::c;

fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

fn arg_obj(args: &[Value], i: usize) -> Result<u16, JvmError> {
    match args.get(i) {
        Some(Value::ObjectRef(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// One out-of-line field write: some thirty of them follow, and each inline
/// copy of `set_field`'s bounds checks was a dozen instructions of flash.
#[inline(never)]
fn put(objects: &mut pico_jvm::object_heap::ObjectHeap, obj: u16, slot: usize, v: Value) {
    objects.set_field(obj, slot, v);
}

/// `LayoutInflater.nativeInflateNode(View v, int layout, int pos, int parentHandle, int
/// parentKind, int[] deferred)`: `true` with the node made and every field written, or `false`
/// with nothing made and the read position untouched, when the node's class takes the Java
/// path (a class this path does not cover, a plain `View` to measure, more attributes to hand
/// back than fit, or LVGL's pool refusing the widget).
///
/// Nothing is allocated after the widget exists: an allocation that fails inside a native
/// means a collection and the native run again, which would make the widget twice.
pub fn layout_inflater_inflate_node(
    ctx: &mut NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let inflater = arg_obj(ctx.args, 0)?;
    let view = arg_obj(ctx.args, 1)?;
    let layout_id = arg_int(ctx.args, 2)?;
    let pos = arg_int(ctx.args, 3)?;
    let parent = arg_int(ctx.args, 4)?;
    let kind = lvgl_inflate::ParentKind::from_java(arg_int(ctx.args, 5)?);
    let deferred = match ctx.args.get(6) {
        Some(Value::ArrayRef(a)) => *a,
        _ => return Err(JvmError::InvalidReference),
    };
    let Some(layout) = crate::resources::layout(layout_id) else {
        return Err(crate::native_handler::res::not_found(
            ctx, "Layout", layout_id,
        ));
    };
    let Ok(pos) = usize::try_from(pos) else {
        return Err(JvmError::InvalidReference);
    };
    let Ok(node) = scan::scan(|i| layout.word(i), pos) else {
        return Err(crate::native_handler::res::not_found(
            ctx, "Layout", layout_id,
        ));
    };
    let declined = Ok(Some(Value::Int(0)));
    if !scan::is_fast_class(node.cls)
        || node.too_many_deferred
        || (scan::is_drawing_class(node.cls)
            && (node.width == scan::WRAP_CONTENT || node.height == scan::WRAP_CONTENT))
        || (scan::is_drawing_class(node.cls) && !cfg!(has_canvas))
    {
        return declined;
    }
    // The text first, interned before anything is made (see above). Its
    // bytes live in the resource table, so the `&str` outlives the intern.
    let mut text: &str = "";
    let mut text_ref: Option<u16> = None;
    if let Some(id) = node.text {
        let Some(bytes) = crate::resources::string(id) else {
            return Err(crate::native_handler::res::not_found(ctx, "String", id));
        };
        text = core::str::from_utf8(bytes).unwrap_or("");
        text_ref = Some(
            ctx.strings
                .intern_dyn(bytes)
                .ok_or(JvmError::StackOverflow)?,
        );
    }
    let Some(parent_obj) = lvgl_inflate::parent_of(parent) else {
        return Err(crate::native_handler::throw_exception(
            ctx,
            c::java_lang_IllegalStateException,
            "inflate: the parent's widget is gone: its Activity finished; build the view again",
        ));
    };
    let made = lvgl_inflate::make(&node, parent_obj, kind, text);
    if made.handle == 0 {
        return declined;
    }
    crate::ui_thread::warn_if_off_ui_thread();

    // The wrapper: its handle and the fields its getters read.
    let objects = &mut *ctx.objects;
    let set = |objects: &mut pico_jvm::object_heap::ObjectHeap, slot: usize, v: Value| {
        put(objects, view, slot, v);
    };
    set(
        objects,
        fields::view::NATIVE_HANDLE,
        Value::Int(made.handle),
    );
    if let Some(id) = node.id {
        set(objects, fields::view::ID, Value::Int(id));
    }
    if let Some(v) = node.visibility {
        set(objects, fields::view::VISIBILITY, Value::Int(v));
    }
    if let Some(e) = node.enabled {
        set(objects, fields::view::ENABLED, Value::Int(e as i32));
    }
    set(
        objects,
        fields::view::FOCUSABLE,
        Value::Int(made.focusable as i32),
    );
    if let Some(k) = node.keep_screen_on {
        set(objects, fields::view::KEEP_SCREEN_ON, Value::Int(k as i32));
    }
    if let Some(w) = node.min_width {
        set(objects, fields::view::MIN_WIDTH, Value::Int(w));
    }
    if let Some(h) = node.min_height {
        set(objects, fields::view::MIN_HEIGHT, Value::Int(h));
    }
    if let Some(a) = node.alpha {
        set(objects, fields::view::ALPHA, Value::Float(a));
    }
    if scan::is_text_class(node.cls) {
        if let Some(r) = text_ref {
            set(objects, fields::text_view::TEXT, Value::Reference(r));
        }
        if let Some(argb) = node.text_color {
            set(
                objects,
                fields::text_view::TEXT_COLOR,
                Value::Int(argb as i32),
            );
        }
        if let Some(px) = node.text_size {
            set(objects, fields::text_view::TEXT_SIZE, Value::Float(px));
        }
        if let Some(g) = node.gravity {
            set(
                objects,
                fields::text_view::GRAVITY,
                Value::Int(scan::text_gravity(g)),
            );
        }
        if let Some(w) = node.max_width {
            set(objects, fields::text_view::MAX_WIDTH, Value::Int(w));
        }
        set(
            objects,
            fields::text_view::LINE_MODE,
            Value::Int(made.line_mode),
        );
    }

    // The inflater: where to read next, and the LayoutParams words.
    use fields::layout_inflater as li;
    let set_i = |objects: &mut pico_jvm::object_heap::ObjectHeap, slot: usize, v: i32| {
        put(objects, inflater, slot, Value::Int(v));
    };
    set_i(objects, li::POS, node.next_pos as i32);
    set_i(objects, li::CHILD_COUNT, node.child_count as i32);
    set_i(objects, li::WIDTH, node.width);
    set_i(objects, li::HEIGHT, node.height);
    put(
        objects,
        inflater,
        li::WEIGHT,
        Value::Float(f32::from_bits(node.weight_bits as u32)),
    );
    set_i(objects, li::LAYOUT_GRAVITY, node.layout_gravity);
    set_i(objects, li::MARGIN_LEFT, node.margin[0]);
    set_i(objects, li::MARGIN_TOP, node.margin[1]);
    set_i(objects, li::MARGIN_RIGHT, node.margin[2]);
    set_i(objects, li::MARGIN_BOTTOM, node.margin[3]);
    set_i(objects, li::DEFERRED_COUNT, node.deferred_count as i32);
    for (i, (attr, value)) in node.deferred[..node.deferred_count].iter().enumerate() {
        ctx.arrays.store(deferred, 2 * i, *attr);
        ctx.arrays.store(deferred, 2 * i + 1, *value);
    }
    Ok(Some(Value::Int(1)))
}
