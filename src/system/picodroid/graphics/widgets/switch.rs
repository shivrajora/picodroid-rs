use crate::lvgl_ffi::*;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::extract_native_handle;

// ---------------------------------------------------------------------------
// Checked-change event queue (ring buffer)
// ---------------------------------------------------------------------------

const SW_CHECKED_CHANGE_QUEUE_SIZE: usize = 16;
static mut SW_CHECKED_CHANGE_QUEUE: [usize; SW_CHECKED_CHANGE_QUEUE_SIZE] =
    [0; SW_CHECKED_CHANGE_QUEUE_SIZE];
static mut SW_CHECKED_CHANGE_QUEUE_HEAD: usize = 0;
static mut SW_CHECKED_CHANGE_QUEUE_TAIL: usize = 0;

// ---------------------------------------------------------------------------
// Handle → Java object mapping (for event dispatch)
// ---------------------------------------------------------------------------

const SW_MAX_LISTENERS: usize = 32;
static mut SW_CHECKED_CHANGE_HANDLE_MAP: [(usize, u16); SW_MAX_LISTENERS] =
    [(0, 0); SW_MAX_LISTENERS];
static mut SW_CHECKED_CHANGE_HANDLE_MAP_LEN: usize = 0;

// ---------------------------------------------------------------------------
// LVGL callback
// ---------------------------------------------------------------------------

/// Called by LVGL when the switch's checked state changes (user tap).
///
/// # Safety
/// Called from LVGL's event dispatch during `lv_timer_handler()`.
unsafe extern "C" fn switch_value_changed_cb(e: *mut lv_event_t) {
    let obj = lv_event_get_target_obj(e);

    let next = (SW_CHECKED_CHANGE_QUEUE_HEAD + 1) % SW_CHECKED_CHANGE_QUEUE_SIZE;
    if next != SW_CHECKED_CHANGE_QUEUE_TAIL {
        SW_CHECKED_CHANGE_QUEUE[SW_CHECKED_CHANGE_QUEUE_HEAD] = obj as usize;
        SW_CHECKED_CHANGE_QUEUE_HEAD = next;
    }
}

// ---------------------------------------------------------------------------
// Native method implementations
// ---------------------------------------------------------------------------

/// `Switch.nativeCreate()` — creates an `lv_switch`.
pub fn switch_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe { lv_switch_create(engine::screen()) };

    unsafe {
        lv_obj_add_event_cb(
            ptr,
            Some(switch_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
    }

    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `Switch.isChecked()`
pub fn switch_is_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = unsafe { lv_obj_has_state(handle_table::lookup(id), LV_STATE_CHECKED) };
    Ok(Some(Value::Int(if checked { 1 } else { 0 })))
}

/// `Switch.setChecked(boolean checked)`
pub fn switch_set_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
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

/// `Switch.toggle()`
pub fn switch_toggle(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    unsafe {
        let obj = handle_table::lookup(id);
        let checked = lv_obj_has_state(obj, LV_STATE_CHECKED);
        if checked {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
    }
    Ok(None)
}

/// `Switch.nativeRegisterCheckedChangeListener()` — records the mapping
/// from this switch's LVGL handle to its Java heap index.
pub fn switch_register_checked_change_listener(
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
        // Update if already registered.
        for entry in &mut SW_CHECKED_CHANGE_HANDLE_MAP[..SW_CHECKED_CHANGE_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        // New registration.
        if SW_CHECKED_CHANGE_HANDLE_MAP_LEN < SW_MAX_LISTENERS {
            SW_CHECKED_CHANGE_HANDLE_MAP[SW_CHECKED_CHANGE_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            SW_CHECKED_CHANGE_HANDLE_MAP_LEN += 1;
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Event queue drain (called from lifecycle event loop)
// ---------------------------------------------------------------------------

/// Pop one checked-change event from the queue, returning the raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_sw_checked_change_queue() -> Option<usize> {
    unsafe {
        if SW_CHECKED_CHANGE_QUEUE_TAIL == SW_CHECKED_CHANGE_QUEUE_HEAD {
            return None;
        }
        let handle = SW_CHECKED_CHANGE_QUEUE[SW_CHECKED_CHANGE_QUEUE_TAIL];
        SW_CHECKED_CHANGE_QUEUE_TAIL =
            (SW_CHECKED_CHANGE_QUEUE_TAIL + 1) % SW_CHECKED_CHANGE_QUEUE_SIZE;
        Some(handle)
    }
}

/// Look up the Java object heap index for a switch given its raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_sw_checked_change_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &SW_CHECKED_CHANGE_HANDLE_MAP[..SW_CHECKED_CHANGE_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Reset all switch state between app runs.
pub fn reset_switch_state() {
    unsafe {
        SW_CHECKED_CHANGE_HANDLE_MAP_LEN = 0;
        SW_CHECKED_CHANGE_QUEUE_HEAD = 0;
        SW_CHECKED_CHANGE_QUEUE_TAIL = 0;
    }
}
