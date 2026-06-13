// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.app.AlertDialog`.

use pico_jvm::heap::StringTable;
use pico_jvm::types::{JvmError, Value};

use super::super::fields;
use super::super::lvgl::widgets::alert_dialog as lvgl_dialog;
use super::super::view::extract_string_at;

pub use lvgl_dialog::reset_alert_dialog_state;
pub use lvgl_dialog::{dismiss_topmost_dialog, has_shown_dialog};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_dialog::{drain_click_queue, drain_item_click_queue, lookup_dialog_obj};

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `AlertDialog.nativeCreate(String title, String message, String pos, String neg, String neu) -> int`
pub fn alert_dialog_native_create(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let title = extract_string_at(args, 0, strings).unwrap_or("");
    let message = extract_string_at(args, 1, strings).unwrap_or("");
    let pos = extract_string_at(args, 2, strings).unwrap_or("");
    let neg = extract_string_at(args, 3, strings).unwrap_or("");
    let neu = extract_string_at(args, 4, strings).unwrap_or("");
    Ok(Some(Value::Int(lvgl_dialog::create(
        title, message, pos, neg, neu,
    ))))
}

/// `AlertDialog.nativeCreateWithList(title, message, pos, neg, neu, itemsJoined, mode, checkedMask) -> int`
pub fn alert_dialog_native_create_with_list(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let title = extract_string_at(args, 0, strings).unwrap_or("");
    let message = extract_string_at(args, 1, strings).unwrap_or("");
    let pos = extract_string_at(args, 2, strings).unwrap_or("");
    let neg = extract_string_at(args, 3, strings).unwrap_or("");
    let neu = extract_string_at(args, 4, strings).unwrap_or("");
    let items = extract_string_at(args, 5, strings).unwrap_or("");
    let mode = arg_int(args, 6)?;
    let checked_mask = arg_int(args, 7)?;
    Ok(Some(Value::Int(lvgl_dialog::create_with_list(
        title,
        message,
        pos,
        neg,
        neu,
        items,
        mode,
        checked_mask,
    ))))
}

/// `AlertDialog.nativePerformItemClick(int handle, int position)` — synthetic
/// list-row click for headless testing.
pub fn alert_dialog_native_perform_item_click(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    let position = arg_int(args, 1)?;
    lvgl_dialog::perform_item_click(id, position);
    Ok(None)
}

/// `AlertDialog.nativeShow(int handle)` — static, takes handle explicitly.
pub fn alert_dialog_native_show(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    lvgl_dialog::show(id);
    Ok(None)
}

/// `AlertDialog.nativeDismiss(int handle)` — static, takes handle explicitly.
pub fn alert_dialog_native_dismiss(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let id = arg_int(args, 0)?;
    lvgl_dialog::dismiss(id);
    Ok(None)
}

/// `AlertDialog.nativeRegisterButtonClickListener()` — instance method;
/// records `this` as the click-listener target keyed by this dialog's
/// `nativeHandle`.
pub fn alert_dialog_register_button_click_listener(
    args: &[Value],
    objects: &pico_jvm::object_heap::ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = match objects.get_field(obj_ref, fields::alert_dialog::NATIVE_HANDLE) {
        Some(Value::Int(h)) => h,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_dialog::register_button_click_listener(id, obj_ref);
    Ok(None)
}
