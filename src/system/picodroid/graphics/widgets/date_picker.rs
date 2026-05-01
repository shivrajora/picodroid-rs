//! Java-binding shim for `picodroid.widget.DatePicker`.

use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::date_picker as lvgl_date_picker;
use super::super::view::extract_native_handle;

pub use lvgl_date_picker::reset_date_picker_state;
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use lvgl_date_picker::{drain_date_picker_queue, lookup_date_picker_obj};

#[inline]
fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

pub fn date_picker_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_date_picker::create())))
}

pub fn date_picker_set_date(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let year = arg_int(args, 1)?;
    let month = arg_int(args, 2)?;
    let day = arg_int(args, 3)?;
    lvgl_date_picker::set_date(id, year, month, day);
    Ok(None)
}

pub fn date_picker_get_year(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let (y, _, _) = lvgl_date_picker::get_date(id);
    Ok(Some(Value::Int(y)))
}

pub fn date_picker_get_month(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let (_, m, _) = lvgl_date_picker::get_date(id);
    Ok(Some(Value::Int(m)))
}

pub fn date_picker_get_day(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let (_, _, d) = lvgl_date_picker::get_date(id);
    Ok(Some(Value::Int(d)))
}

pub fn date_picker_register_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_date_picker::register_listener(id, obj_ref);
    Ok(None)
}
