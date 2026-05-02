// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ScrollView`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::gfx::Handle;
use super::super::lvgl::widgets::scroll_view as lvgl_scroll_view;
use super::super::lvgl::with_gfx;
use super::super::view::{extract_handle_at, extract_native_handle};

pub fn scroll_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_scroll_view::create())))
}

pub fn scroll_view_add_view(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let parent_id = extract_native_handle(args, objects)?;
    let child_id = extract_handle_at(args, 1, objects)?;
    with_gfx(|g| g.set_parent(Handle::from_java(child_id), Handle::from_java(parent_id)));
    Ok(None)
}
