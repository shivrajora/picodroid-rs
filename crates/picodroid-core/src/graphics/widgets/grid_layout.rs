// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.GridLayout`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::grid_layout as lvgl_grid_layout;
use super::super::view::{arg_int, extract_handle_at, extract_native_handle};

pub fn grid_layout_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_grid_layout::create())))
}

/// `GridLayout.nativePlace(View child, int colAlign, int col, int colSpan, int colFr,
/// int rowAlign, int row, int rowSpan, int rowFr)`: `true` when the cell was taken,
/// `false` when the grid has no slot or the cell lies past the last track.
pub fn grid_layout_place(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let child = extract_handle_at(args, 1, objects)?;
    let spec = lvgl_grid_layout::CellSpec {
        col_align: arg_int(args, 2)?,
        col: arg_int(args, 3)?,
        col_span: arg_int(args, 4)?,
        col_fr: arg_int(args, 5)?,
        row_align: arg_int(args, 6)?,
        row: arg_int(args, 7)?,
        row_span: arg_int(args, 8)?,
        row_fr: arg_int(args, 9)?,
    };
    let placed = lvgl_grid_layout::place(id, child, spec);
    Ok(Some(Value::Int(placed as i32)))
}
