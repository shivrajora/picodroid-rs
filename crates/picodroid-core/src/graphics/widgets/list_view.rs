// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ListView`.
//!
//! Rows come from Java: `ListView.refreshFromAdapter` calls the adapter's
//! `getView` itself and adds each row as a child, then `nativeStyleRow` gives
//! it a list row's behaviour. The earlier native→Java `nativeBindAdapter`
//! loop (the T2.5 upcall proof) went with the row-data-only list; the
//! upcall itself stays in use elsewhere (`Comparator.compare`, `toString`).
//! A long list keeps a window of rows and slides it on `fireScroll`, the
//! deferred callback the scroll queue below feeds; the natives after
//! `list_view_style_row` are the window's LVGL half
//! (docs/designs/listview-recycling-2026-10.md).

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::list_view as lvgl_list_view;
use super::super::view::{extract_handle_at, extract_native_handle, extract_string_at};
use super::super::view::{register_listener_with, NO_ATTACH};

pub use lvgl_list_view::reset_list_view_state;
pub use lvgl_list_view::{drain_item_click_queue, drain_scroll_queue};

fn int_at(args: &[Value], idx: usize) -> Result<i32, JvmError> {
    match args.get(idx) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

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
    register_listener_with(args, objects, NO_ATTACH)
}

/// `ListView.nativeStyleRow(View row, int height)`: the row is already a
/// child of the list (`addView` reparented it); this makes it look and act
/// like a row, pinned to `height` when that is positive.
pub fn list_view_style_row(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let row = extract_handle_at(args, 1, objects)?;
    let height = int_at(args, 2)?;
    lvgl_list_view::style_row(row, height);
    Ok(None)
}

/// `ListView.nativeWatchViewport()`: queue `fireScroll` on scroll and resize.
pub fn list_view_watch_viewport(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_list_view::watch_viewport(id);
    Ok(None)
}

/// `ListView.nativeScrollY()`: the content's scroll offset in pixels.
pub fn list_view_scroll_y(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    Ok(Some(Value::Int(lvgl_list_view::scroll_y(id))))
}

/// `ListView.nativeScrollTo(int y, boolean smooth)`.
pub fn list_view_scroll_to(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let y = int_at(args, 1)?;
    let smooth = int_at(args, 2)? != 0;
    lvgl_list_view::scroll_to_y(id, y, smooth);
    Ok(None)
}

/// `ListView.nativeSetSpacers(int top, int bottom)`: the padding standing in
/// for the rows outside the window, in pixels.
pub fn list_view_set_spacers(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let top = int_at(args, 1)?;
    let bottom = int_at(args, 2)?;
    lvgl_list_view::set_spacers(id, top, bottom);
    Ok(None)
}

/// `ListView.nativeRecycleRow(View row, boolean toEnd)`: move a row from
/// one end of the window to the other, handing the keypad focus on first.
pub fn list_view_recycle_row(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let row = extract_handle_at(args, 1, objects)?;
    let to_end = int_at(args, 2)? != 0;
    lvgl_list_view::recycle_row(row, to_end);
    Ok(None)
}

/// `ListView.nativeSyncFocusOrder()`: the keypad group's rows in child order.
pub fn list_view_sync_focus_order(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_list_view::sync_focus_order(id);
    Ok(None)
}
