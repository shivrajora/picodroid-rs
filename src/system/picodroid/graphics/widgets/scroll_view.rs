use crate::lvgl_ffi::*;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_handle_at, extract_native_handle};

/// `ScrollView.nativeCreate()` -- creates a scrollable `lv_obj` container.
/// LVGL objects scroll by default when content exceeds bounds.
pub fn scroll_view_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe {
        let o = lv_obj_create(engine::screen());
        // Clear theme-applied padding so the scroll container is transparent;
        // padding is controlled explicitly by the app via setPadding().
        lv_obj_set_style_pad_left(o, 0, 0);
        lv_obj_set_style_pad_right(o, 0, 0);
        lv_obj_set_style_pad_top(o, 0, 0);
        lv_obj_set_style_pad_bottom(o, 0, 0);
        o
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `ScrollView.addView(View child)`
pub fn scroll_view_add_view(
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
