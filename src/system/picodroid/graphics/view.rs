//! Native method implementations for `picodroid.view.View`.

use crate::lvgl_ffi::*;
use core::ffi::c_char;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::handle_table;

// ---------------------------------------------------------------------------
// Helpers shared across view.rs and widgets.rs
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

/// Look up the `lv_obj_t*` for a handle ID from `extract_native_handle`.
pub fn resolve(id: i32) -> *mut lv_obj_t {
    handle_table::lookup(id)
}

/// Convert a Java string `Reference` to a null-terminated byte buffer on the stack.
///
/// Returns a `c_char` pointer valid for the lifetime of `buf`.
/// LVGL text APIs copy the string internally, so the pointer need not outlive the call.
/// Strings longer than 127 bytes are truncated.
pub fn java_str_to_cstr(
    arg: &Value,
    strings: &StringTable,
    buf: &mut [u8; 128],
) -> Result<*const c_char, JvmError> {
    let idx = match arg {
        Value::Reference(idx) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let s = strings.resolve(idx).ok_or(JvmError::InvalidReference)?;
    let bytes = s.as_bytes();
    let len = bytes.len().min(127);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
    Ok(buf.as_ptr() as *const c_char)
}

/// Convert an ARGB `int` to an `lv_color_t` (RGB888, ignoring alpha).
fn argb_to_lv_color(argb: i32) -> lv_color_t {
    lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    }
}

// ---------------------------------------------------------------------------
// View native methods
// ---------------------------------------------------------------------------

/// `View.setPosition(int x, int y)`
pub fn set_position(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let x = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let y = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_obj_set_pos(resolve(id), x, y) };
    Ok(None)
}

/// `View.setSize(int width, int height)`
pub fn set_size(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let w = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let h = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_obj_set_size(resolve(id), w, h) };
    Ok(None)
}

/// `View.setBackgroundColor(int argb)`
pub fn set_bg_color(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let color = argb_to_lv_color(argb);
    unsafe { lv_obj_set_style_bg_color(resolve(id), color, 0) };
    Ok(None)
}

/// `View.setVisibility(int visibility)`
pub fn set_visibility(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let vis = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = resolve(id);
        if vis == 0 {
            // VISIBLE
            lv_obj_remove_flag(obj, LV_OBJ_FLAG_HIDDEN);
        } else {
            // INVISIBLE or GONE
            lv_obj_add_flag(obj, LV_OBJ_FLAG_HIDDEN);
        }
    }
    Ok(None)
}

/// `View.setPadding(int left, int top, int right, int bottom)`
pub fn set_padding(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let left = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let top = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let right = match args.get(3) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let bottom = match args.get(4) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = resolve(id);
        lv_obj_set_style_pad_left(obj, left, 0);
        lv_obj_set_style_pad_top(obj, top, 0);
        lv_obj_set_style_pad_right(obj, right, 0);
        lv_obj_set_style_pad_bottom(obj, bottom, 0);
    }
    Ok(None)
}

/// `View.setEnabled(boolean enabled)`
pub fn set_enabled(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let enabled = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = resolve(id);
        if enabled {
            lv_obj_remove_state(obj, LV_STATE_DISABLED);
        } else {
            lv_obj_add_state(obj, LV_STATE_DISABLED);
        }
    }
    Ok(None)
}

/// `View.setAlpha(float alpha)`
pub fn set_alpha(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let alpha = match args.get(1) {
        Some(Value::Float(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let opa = (alpha * 255.0) as u8;
    unsafe { lv_obj_set_style_opa(resolve(id), opa, 0) };
    Ok(None)
}

/// `View.close()`
pub fn close(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    unsafe { lv_obj_delete(resolve(id)) };
    Ok(None)
}
