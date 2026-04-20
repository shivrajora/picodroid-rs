use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_native_handle, java_str_to_cstr};

// ---------------------------------------------------------------------------
// Checked-change event queue (ring buffer)
// ---------------------------------------------------------------------------

const CB_CHECKED_CHANGE_QUEUE_SIZE: usize = 16;
static mut CB_CHECKED_CHANGE_QUEUE: [usize; CB_CHECKED_CHANGE_QUEUE_SIZE] =
    [0; CB_CHECKED_CHANGE_QUEUE_SIZE];
static mut CB_CHECKED_CHANGE_QUEUE_HEAD: usize = 0;
static mut CB_CHECKED_CHANGE_QUEUE_TAIL: usize = 0;

// ---------------------------------------------------------------------------
// Handle -> Java object mapping (for event dispatch)
// ---------------------------------------------------------------------------

const CB_MAX_LISTENERS: usize = 32;
static mut CB_HANDLE_MAP: [(usize, u16); CB_MAX_LISTENERS] = [(0, 0); CB_MAX_LISTENERS];
static mut CB_HANDLE_MAP_LEN: usize = 0;

// ---------------------------------------------------------------------------
// LVGL callback
// ---------------------------------------------------------------------------

/// Called by LVGL when the checkbox's checked state changes (user tap).
///
/// # Safety
/// Called from LVGL's event dispatch during `lv_timer_handler()`.
unsafe extern "C" fn checkbox_value_changed_cb(e: *mut lv_event_t) {
    let obj = lv_event_get_target_obj(e);

    let next = (CB_CHECKED_CHANGE_QUEUE_HEAD + 1) % CB_CHECKED_CHANGE_QUEUE_SIZE;
    if next != CB_CHECKED_CHANGE_QUEUE_TAIL {
        CB_CHECKED_CHANGE_QUEUE[CB_CHECKED_CHANGE_QUEUE_HEAD] = obj as usize;
        CB_CHECKED_CHANGE_QUEUE_HEAD = next;
    }
}

// ---------------------------------------------------------------------------
// Native method implementations
// ---------------------------------------------------------------------------

/// `CheckBox.nativeCreate()` -- creates an `lv_checkbox`.
pub fn check_box_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe {
        let cb = lv_checkbox_create(engine::screen());
        lv_obj_add_event_cb(
            cb,
            Some(checkbox_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        cb
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `CheckBox.setText(String text)`
pub fn check_box_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(
        args.get(1).ok_or(JvmError::InvalidReference)?,
        strings,
        &mut buf,
    )?;
    unsafe { lv_checkbox_set_text(handle_table::lookup(id), cstr) };
    Ok(None)
}

/// `CheckBox.isChecked()`
pub fn check_box_is_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = unsafe { lv_obj_has_state(handle_table::lookup(id), LV_STATE_CHECKED) };
    Ok(Some(Value::Int(if checked { 1 } else { 0 })))
}

/// `CheckBox.setChecked(boolean checked)`
pub fn check_box_set_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = handle_table::lookup(id);
        if checked {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        }
    }
    Ok(None)
}

/// `CheckBox.performCheckedChange()` — synthetically toggle and fire
/// `LV_EVENT_VALUE_CHANGED`. Goes through the same native callback a real
/// tap would trigger, so the registered listener runs on the next
/// dispatch tick.
pub fn check_box_perform_checked_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    unsafe {
        let obj = handle_table::lookup(id);
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
        lv_obj_send_event(obj, LV_EVENT_VALUE_CHANGED, core::ptr::null_mut());
    }
    Ok(None)
}

/// `CheckBox.nativeRegisterCheckedChangeListener()` -- records the mapping
/// from this checkbox's LVGL handle to its Java heap index.
pub fn check_box_register_checked_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    let raw_ptr = handle_table::lookup(id) as usize;

    unsafe {
        for entry in &mut CB_HANDLE_MAP[..CB_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        if CB_HANDLE_MAP_LEN < CB_MAX_LISTENERS {
            CB_HANDLE_MAP[CB_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            CB_HANDLE_MAP_LEN += 1;
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Event queue drain (called from lifecycle event loop)
// ---------------------------------------------------------------------------

/// Pop one checked-change event from the queue.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_cb_checked_change_queue() -> Option<usize> {
    unsafe {
        if CB_CHECKED_CHANGE_QUEUE_TAIL == CB_CHECKED_CHANGE_QUEUE_HEAD {
            return None;
        }
        let handle = CB_CHECKED_CHANGE_QUEUE[CB_CHECKED_CHANGE_QUEUE_TAIL];
        CB_CHECKED_CHANGE_QUEUE_TAIL =
            (CB_CHECKED_CHANGE_QUEUE_TAIL + 1) % CB_CHECKED_CHANGE_QUEUE_SIZE;
        Some(handle)
    }
}

/// Look up the Java object heap index for a checkbox given its raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_cb_checked_change_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &CB_HANDLE_MAP[..CB_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Reset all checkbox state between app runs.
pub fn reset_check_box_state() {
    unsafe {
        CB_HANDLE_MAP_LEN = 0;
        CB_CHECKED_CHANGE_QUEUE_HEAD = 0;
        CB_CHECKED_CHANGE_QUEUE_TAIL = 0;
    }
}
