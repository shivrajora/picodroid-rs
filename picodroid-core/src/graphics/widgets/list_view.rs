// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ListView`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::list_view as lvgl_list_view;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_list_view::reset_list_view_state;
// `visit_item_click_listener_roots` is reached directly via the lvgl path in
// `gc_visit_roots` (mirroring `button::visit_click_listener_roots`), so it is
// not re-exported here.
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_list_view::{drain_item_click_queue, lookup_item_click};

pub fn list_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_list_view::create())))
}

pub fn list_view_add_item(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_list_view::add_item(id, text);
    Ok(None)
}

pub fn list_view_register_item_click_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_list_view::register_item_click_listener(id, obj_ref);
    Ok(None)
}
