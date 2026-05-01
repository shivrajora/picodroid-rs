//! Java-binding shim for `picodroid.widget.SwipeRefreshLayout`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::swipe_refresh_layout as lvgl_swipe_refresh;
use super::super::view::extract_native_handle;

pub use lvgl_swipe_refresh::reset_swipe_refresh_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_swipe_refresh::{drain_refresh_queue, lookup_refresh_obj};

pub fn swipe_refresh_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_swipe_refresh::create())))
}

pub fn swipe_refresh_add_view(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let parent = extract_native_handle(args, objects)?;
    let child = match args.get(1) {
        Some(Value::ObjectRef(idx)) => {
            match objects.get_field(*idx, super::super::fields::view::NATIVE_HANDLE) {
                Some(Value::Int(h)) => h,
                _ => return Err(JvmError::InvalidReference),
            }
        }
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_swipe_refresh::add_view(parent, child);
    Ok(None)
}

pub fn swipe_refresh_set_refreshing(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let refreshing = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_swipe_refresh::set_refreshing(id, refreshing);
    Ok(None)
}

pub fn swipe_refresh_register_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_swipe_refresh::register_listener(id, obj_ref);
    Ok(None)
}
