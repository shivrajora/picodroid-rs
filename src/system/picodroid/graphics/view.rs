//! Native method implementations for `picodroid.view.View`.
//!
//! Cross-widget setters route through the [`super::gfx::Gfx`] trait via
//! `with_gfx`; the LVGL specifics live in `lvgl::view_ops`. The helpers
//! (`extract_native_handle`, `java_str_to_cstr`, `resolve`) and the key
//! listener registry stay here because they bridge the JVM heap to the
//! widget call sites — they are not LVGL-specific in a way the trait
//! could hide cleanly.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::gfx::{Handle, Visibility};
use super::lvgl::events as lvgl_events;
use super::lvgl::with_gfx;

// ---------------------------------------------------------------------------
// Helpers shared with widgets/*.rs
// ---------------------------------------------------------------------------

/// Extract the `nativeHandle` ID from `this` (args[0]).
pub fn extract_native_handle(args: &[Value], objects: &ObjectHeap) -> Result<i32, JvmError> {
    let obj_idx = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    match objects.get_field(obj_idx, fields::view::NATIVE_HANDLE) {
        Some(Value::Int(handle)) => Ok(handle),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Extract the `nativeHandle` ID from a `View` argument at the given position.
pub fn extract_handle_at(
    args: &[Value],
    arg_idx: usize,
    objects: &ObjectHeap,
) -> Result<i32, JvmError> {
    let obj_idx = match args.get(arg_idx) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    match objects.get_field(obj_idx, fields::view::NATIVE_HANDLE) {
        Some(Value::Int(handle)) => Ok(handle),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Extract a Java `String` argument as a Rust `&str`. Returned reference
/// is valid for the lifetime of `strings`.
pub fn extract_string_at<'s>(
    args: &[Value],
    arg_idx: usize,
    strings: &'s StringTable,
) -> Result<&'s str, JvmError> {
    let idx = match args.get(arg_idx) {
        Some(Value::Reference(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    strings.resolve(idx).ok_or(JvmError::InvalidReference)
}

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

#[inline]
fn arg_float(args: &[Value], i: usize) -> Result<f32, JvmError> {
    match args.get(i) {
        Some(Value::Float(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

// ---------------------------------------------------------------------------
// View native methods — all routed through Gfx::* via with_gfx.
// ---------------------------------------------------------------------------

/// `View.setPosition(int x, int y)`
pub fn set_position(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let x = arg_int(args, 1)?;
    let y = arg_int(args, 2)?;
    with_gfx(|g| g.set_pos(Handle::from_java(id), x, y));
    Ok(None)
}

/// `View.setSize(int width, int height)`
pub fn set_size(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let w = arg_int(args, 1)?;
    let h = arg_int(args, 2)?;
    with_gfx(|g| g.set_size(Handle::from_java(id), w, h));
    Ok(None)
}

/// `View.setBackgroundColor(int argb)`
pub fn set_bg_color(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = arg_int(args, 1)? as u32;
    with_gfx(|g| g.set_bg_color(Handle::from_java(id), argb));
    Ok(None)
}

/// `View.setVisibility(int visibility)` — Android constants
/// (0=VISIBLE, 1=INVISIBLE, 2=GONE).
pub fn set_visibility(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let v = match arg_int(args, 1)? {
        0 => Visibility::Visible,
        1 => Visibility::Invisible,
        _ => Visibility::Gone,
    };
    with_gfx(|g| g.set_visibility(Handle::from_java(id), v));
    Ok(None)
}

/// `View.setPadding(int left, int top, int right, int bottom)`
pub fn set_padding(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let left = arg_int(args, 1)?;
    let top = arg_int(args, 2)?;
    let right = arg_int(args, 3)?;
    let bottom = arg_int(args, 4)?;
    with_gfx(|g| g.set_padding(Handle::from_java(id), left, top, right, bottom));
    Ok(None)
}

/// `View.setEnabled(boolean enabled)`
pub fn set_enabled(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let on = arg_int(args, 1)? != 0;
    with_gfx(|g| g.set_enabled(Handle::from_java(id), on));
    Ok(None)
}

/// `View.setAlpha(float alpha)`
pub fn set_alpha(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let alpha = (arg_float(args, 1)? * 255.0) as u8;
    with_gfx(|g| g.set_alpha(Handle::from_java(id), alpha));
    Ok(None)
}

/// `View.close()`
pub fn close(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    with_gfx(|g| g.delete(Handle::from_java(id)));
    Ok(None)
}

// ---------------------------------------------------------------------------
// Key-listener Java bindings — registry lives in `lvgl::events`.
// ---------------------------------------------------------------------------

/// `View.nativeRegisterKeyListener()` — records this View as a key-listener
/// candidate so the framework event loop can dispatch `fireKey()` when this
/// widget is LVGL-focused.
pub fn register_key_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_events::register_view_key_listener(id, obj_ref);
    Ok(None)
}

/// `View.nativeRegisterTouchListener()` — records this View as a
/// touch-listener target and wires the LVGL press/release/long-press
/// callbacks. Side effect: flips on `LV_OBJ_FLAG_CLICKABLE` so passive
/// widgets (TextView, layouts) actually receive hit-tested events.
pub fn register_touch_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_events::register_view_touch_listener(id, obj_ref);
    Ok(None)
}

/// `View.nativeRegisterSwipeListener()` — records this View as a
/// swipe-gesture target and registers the LVGL `LV_EVENT_GESTURE` callback
/// on the underlying object. Note: the widget itself doesn't need to be
/// clickable for gestures to fire (LVGL routes gestures via the indev,
/// not the hit-test path), so we don't touch the CLICKABLE flag here.
pub fn register_swipe_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_events::register_view_swipe_listener(id, obj_ref);
    Ok(None)
}

/// Reset the key-listener registry between app runs.
pub fn reset_key_listener_state() {
    lvgl_events::reset_view_key_listener_state();
}

/// Reset the touch-listener registry between app runs.
pub fn reset_touch_listener_state() {
    lvgl_events::reset_view_touch_listener_state();
}

/// Reset the swipe-listener registry between app runs.
pub fn reset_swipe_listener_state() {
    lvgl_events::reset_view_swipe_listener_state();
}
