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
use super::handle_table;
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
// Key-listener registry (lv_obj_t* → Java View ObjectRef)
// ---------------------------------------------------------------------------

const MAX_KEY_LISTENERS: usize = 32;
/// Maps LVGL `lv_obj_t*` to the Java `View` heap index that registered a key
/// listener on it. The listener object itself is read back from the View's
/// `onKeyListener` field at dispatch time (so stale references are impossible).
static mut VIEW_KEY_MAP: [(usize, u16); MAX_KEY_LISTENERS] = [(0, 0); MAX_KEY_LISTENERS];
static mut VIEW_KEY_MAP_LEN: usize = 0;

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
    let raw_ptr = handle_table::lookup(id) as usize;

    unsafe {
        for entry in &mut VIEW_KEY_MAP[..VIEW_KEY_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        if VIEW_KEY_MAP_LEN < MAX_KEY_LISTENERS {
            VIEW_KEY_MAP[VIEW_KEY_MAP_LEN] = (raw_ptr, obj_ref);
            VIEW_KEY_MAP_LEN += 1;
        }
    }
    Ok(None)
}

/// Look up the Java `View` object heap index for a given LVGL handle.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_view_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &VIEW_KEY_MAP[..VIEW_KEY_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Reset the key-listener registry between app runs.
pub fn reset_key_listener_state() {
    unsafe {
        VIEW_KEY_MAP_LEN = 0;
    }
}
