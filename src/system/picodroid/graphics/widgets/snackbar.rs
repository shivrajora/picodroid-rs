//! Java-binding shim for `picodroid.widget.Snackbar`.
//!
//! Snackbar is *not* a `View` subclass (no `nativeHandle` field on a View
//! superclass). Static native methods receive the handle as an explicit
//! `int` argument; the only instance method, `nativeRegisterActionClickListener`,
//! reads `nativeHandle` off the receiver via the Snackbar field map.

use pico_jvm::heap::StringTable;
use pico_jvm::types::{JvmError, Value};

use super::super::fields;
use super::super::lvgl::widgets::snackbar as lvgl_snackbar;
use super::super::view::extract_string_at;

pub use lvgl_snackbar::reset_snackbar_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_snackbar::{drain_click_queue as drain_snackbar_click_queue, lookup_snackbar_obj};

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `Snackbar.nativeCreate(String text, int duration) -> int handle`
pub fn snackbar_native_create(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let text = extract_string_at(args, 0, strings).unwrap_or("");
    let duration = arg_int(args, 1)?;
    Ok(Some(Value::Int(lvgl_snackbar::create(text, duration))))
}

/// `Snackbar.nativeShow(int handle)`
pub fn snackbar_native_show(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    lvgl_snackbar::show(id);
    Ok(None)
}

/// `Snackbar.nativeDismiss(int handle)`
pub fn snackbar_native_dismiss(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    lvgl_snackbar::dismiss(id);
    Ok(None)
}

/// `Snackbar.nativeSetAction(int handle, String label)`
pub fn snackbar_native_set_action(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    let label = extract_string_at(args, 1, strings).unwrap_or("");
    lvgl_snackbar::set_action(id, label);
    Ok(None)
}

/// `Snackbar.nativeRegisterActionClickListener()` — instance method;
/// records `this` as the action-click target keyed by this snackbar's
/// `nativeHandle`.
pub fn snackbar_register_action_click_listener(
    args: &[Value],
    objects: &pico_jvm::object_heap::ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = match objects.get_field(obj_ref, fields::snackbar::NATIVE_HANDLE) {
        Some(Value::Int(h)) => h,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_snackbar::register_action_click_listener(id, obj_ref);
    Ok(None)
}
