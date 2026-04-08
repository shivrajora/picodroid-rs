use crate::lvgl_ffi::*;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_handle_at, extract_native_handle};

/// `LinearLayout.nativeCreate()` — creates an `lv_obj` with flex column layout.
pub fn linear_layout_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe {
        let o = lv_obj_create(engine::screen());
        lv_obj_set_flex_flow(o, LV_FLEX_FLOW_COLUMN);
        lv_obj_set_flex_align(
            o,
            LV_FLEX_ALIGN_START,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );
        o
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `LinearLayout.addView(View child)`
pub fn linear_layout_add_view(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let parent_id = extract_native_handle(args, objects)?;
    let child_id = extract_handle_at(args, 1, objects)?;
    unsafe {
        lv_obj_set_parent(
            handle_table::lookup(child_id),
            handle_table::lookup(parent_id),
        );
    }
    Ok(None)
}

/// `LinearLayout.setOrientation(int orientation)`
pub fn linear_layout_set_orientation(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let orientation = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let flow = if orientation == 0 {
        LV_FLEX_FLOW_ROW
    } else {
        LV_FLEX_FLOW_COLUMN
    };
    unsafe { lv_obj_set_flex_flow(handle_table::lookup(id), flow) };
    Ok(None)
}
