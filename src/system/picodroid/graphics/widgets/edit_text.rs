//! Java-binding shim for `picodroid.widget.EditText`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::edit_text as lvgl_edit_text;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_edit_text::reset_edit_text_state;

pub fn edit_text_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_edit_text::create())))
}

pub fn edit_text_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_edit_text::set_text(id, text);
    Ok(None)
}

pub fn edit_text_get_text(
    args: &[Value],
    strings: &mut StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let mut buf = [0u8; 256];
    let Some(len) = lvgl_edit_text::get_text(id, &mut buf) else {
        return Ok(Some(Value::Null));
    };
    let ref_idx = strings
        .intern_dyn(&buf[..len])
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(ref_idx)))
}

pub fn edit_text_set_hint(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let hint = extract_string_at(args, 1, strings)?;
    lvgl_edit_text::set_hint(id, hint);
    Ok(None)
}

/// `EditText.setShowKeyboardOnTouch(boolean enabled)`
pub fn edit_text_set_show_keyboard_on_touch(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let enabled = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_edit_text::set_autoshow(id, enabled);
    Ok(None)
}

/// `EditText.nativeRegisterEditorActionListener()` — instance method.
/// Records this EditText's Java `obj_ref` so the system keyboard's OK key
/// can dispatch `fireEditorAction` back to it.
pub fn edit_text_register_editor_action_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_edit_text::register_editor_action_listener(id, obj_ref);
    Ok(None)
}
