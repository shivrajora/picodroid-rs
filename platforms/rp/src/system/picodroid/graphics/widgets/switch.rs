// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.Switch`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::switch as lvgl_switch;
use super::super::view::extract_native_handle;

pub use lvgl_switch::reset_switch_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_switch::{drain_sw_checked_change_queue, lookup_sw_checked_change_obj};

pub fn switch_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_switch::create())))
}

pub fn switch_is_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(if lvgl_switch::is_checked(id) {
        1
    } else {
        0
    })))
}

pub fn switch_set_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_switch::set_checked(id, checked);
    Ok(None)
}

pub fn switch_toggle(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_switch::toggle(id);
    Ok(None)
}

pub fn switch_perform_checked_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_switch::perform_checked_change(id);
    Ok(None)
}

pub fn switch_register_checked_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_switch::register_listener(id, obj_ref);
    Ok(None)
}
