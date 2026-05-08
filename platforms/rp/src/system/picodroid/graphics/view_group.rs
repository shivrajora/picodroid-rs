// SPDX-License-Identifier: GPL-3.0-only
//! Native method implementations for `picodroid.view.ViewGroup`.
//!
//! ViewGroup is the abstract parent of LinearLayout, FrameLayout, ScrollView,
//! SwipeRefreshLayout, and AdapterView. The native methods declared on it
//! (addView/removeView/removeAllViews/getChildCount) are inherited by every
//! layout class and routed here via [`super::super::native_handler::graphics::is_view_group`].

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::gfx::Handle;
use super::lvgl::with_gfx;
use super::view::{extract_handle_at, extract_native_handle};

/// `ViewGroup.addView(View child)`
pub fn add_view(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let parent = extract_native_handle(args, objects)?;
    let child = extract_handle_at(args, 1, objects)?;
    with_gfx(|g| g.set_parent(Handle::from_java(child), Handle::from_java(parent)));
    Ok(None)
}

/// `ViewGroup.removeView(View child)`
pub fn remove_view(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let parent = extract_native_handle(args, objects)?;
    let child = extract_handle_at(args, 1, objects)?;
    with_gfx(|g| g.remove_child(Handle::from_java(parent), Handle::from_java(child)));
    Ok(None)
}

/// `ViewGroup.removeAllViews()`
pub fn remove_all_views(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    with_gfx(|g| g.remove_all_children(Handle::from_java(id)));
    Ok(None)
}

/// `ViewGroup.getChildCount()`
pub fn get_child_count(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let count = with_gfx(|g| g.child_count(Handle::from_java(id)));
    Ok(Some(Value::Int(count)))
}
