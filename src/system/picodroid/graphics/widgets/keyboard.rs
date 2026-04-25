//! Java-binding shim for `picodroid.widget.Keyboard`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::keyboard as lvgl_keyboard;
use super::super::view::{extract_handle_at, extract_native_handle};

pub use lvgl_keyboard::reset_keyboard_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_keyboard::{drain_ready_queue, lookup_keyboard_obj};

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `Keyboard.nativeCreate()` — fresh per-instance LVGL keyboard.
pub fn keyboard_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_keyboard::create())))
}

/// `Keyboard.nativeSetTextarea(EditText target)` — instance method.
/// `args[0]` is `this` (Keyboard); `args[1]` is the EditText. Both are
/// View subclasses, so the handle lives at slot 0 of each.
pub fn keyboard_set_textarea(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let kb_id = extract_native_handle(args, objects)?;
    let ta_id = extract_handle_at(args, 1, objects)?;
    lvgl_keyboard::set_textarea(kb_id, ta_id);
    Ok(None)
}

/// `Keyboard.nativeSetMode(int mode)` — instance method.
pub fn keyboard_set_mode(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let kb_id = extract_native_handle(args, objects)?;
    let mode = arg_int(args, 1)? as u32;
    lvgl_keyboard::set_mode(kb_id, mode);
    Ok(None)
}

/// `Keyboard.nativeRegisterReadyListener()` — instance method.
pub fn keyboard_register_ready_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_keyboard::register_ready_listener(id, obj_ref);
    Ok(None)
}
