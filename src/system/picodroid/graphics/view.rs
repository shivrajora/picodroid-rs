//! Native method implementations for `picodroid.view.View`.

use crate::lvgl_ffi::*;
use core::ffi::c_char;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;

// ---------------------------------------------------------------------------
// Helpers shared across view.rs and widgets.rs
// ---------------------------------------------------------------------------

/// Extract the `lv_obj_t*` handle from `this` (args[0]).
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

/// Extract the `lv_obj_t*` handle from a `View` argument at the given position.
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
    let handle = extract_native_handle(args, objects)?;
    let x = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let y = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_obj_set_pos(handle as *mut lv_obj_t, x, y) };
    Ok(None)
}

/// `View.setSize(int width, int height)`
pub fn set_size(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let w = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let h = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_obj_set_size(handle as *mut lv_obj_t, w, h) };
    Ok(None)
}

/// `View.setBackgroundColor(int argb)`
pub fn set_bg_color(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let color = argb_to_lv_color(argb);
    unsafe { lv_obj_set_style_bg_color(handle as *mut lv_obj_t, color, 0) };
    Ok(None)
}

/// `View.setVisibility(int visibility)`
pub fn set_visibility(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let vis = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = handle as *mut lv_obj_t;
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

/// `View.close()`
pub fn close(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    unsafe { lv_obj_delete(handle as *mut lv_obj_t) };
    Ok(None)
}
