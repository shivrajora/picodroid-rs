use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_native_handle, java_str_to_cstr};

// ---------------------------------------------------------------------------
// Value-changed event queue (ring buffer)
// ---------------------------------------------------------------------------

const SPIN_CHANGE_QUEUE_SIZE: usize = 16;
static mut SPIN_CHANGE_QUEUE: [usize; SPIN_CHANGE_QUEUE_SIZE] = [0; SPIN_CHANGE_QUEUE_SIZE];
static mut SPIN_CHANGE_QUEUE_HEAD: usize = 0;
static mut SPIN_CHANGE_QUEUE_TAIL: usize = 0;

// ---------------------------------------------------------------------------
// Handle -> Java object mapping (for event dispatch)
// ---------------------------------------------------------------------------

const SPIN_MAX_LISTENERS: usize = 32;
static mut SPIN_HANDLE_MAP: [(usize, u16); SPIN_MAX_LISTENERS] = [(0, 0); SPIN_MAX_LISTENERS];
static mut SPIN_HANDLE_MAP_LEN: usize = 0;

// ---------------------------------------------------------------------------
// LVGL callback
// ---------------------------------------------------------------------------

/// Called by LVGL when the dropdown selection changes.
///
/// # Safety
/// Called from LVGL's event dispatch during `lv_timer_handler()`.
unsafe extern "C" fn spinner_value_changed_cb(e: *mut lv_event_t) {
    let obj = lv_event_get_target_obj(e);

    let next = (SPIN_CHANGE_QUEUE_HEAD + 1) % SPIN_CHANGE_QUEUE_SIZE;
    if next != SPIN_CHANGE_QUEUE_TAIL {
        SPIN_CHANGE_QUEUE[SPIN_CHANGE_QUEUE_HEAD] = obj as usize;
        SPIN_CHANGE_QUEUE_HEAD = next;
    }
}

// ---------------------------------------------------------------------------
// Native method implementations
// ---------------------------------------------------------------------------

/// `Spinner.nativeCreate()` -- creates an `lv_dropdown`.
pub fn spinner_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe {
        let dd = lv_dropdown_create(engine::screen());
        lv_obj_add_event_cb(
            dd,
            Some(spinner_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        dd
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `Spinner.setItems(String items)` -- newline-separated option list.
pub fn spinner_set_items(
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
    unsafe { lv_dropdown_set_options(handle_table::lookup(id), cstr) };
    Ok(None)
}

/// `Spinner.getSelectedItemPosition()`
pub fn spinner_get_selected(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let sel = unsafe { lv_dropdown_get_selected(handle_table::lookup(id)) };
    Ok(Some(Value::Int(sel as i32)))
}

/// `Spinner.nativeRegisterItemSelectedListener()` -- records the mapping
/// from this dropdown's LVGL handle to its Java heap index.
pub fn spinner_register_item_selected_listener(
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
        for entry in &mut SPIN_HANDLE_MAP[..SPIN_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        if SPIN_HANDLE_MAP_LEN < SPIN_MAX_LISTENERS {
            SPIN_HANDLE_MAP[SPIN_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            SPIN_HANDLE_MAP_LEN += 1;
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Event queue drain (called from lifecycle event loop)
// ---------------------------------------------------------------------------

/// Pop one value-changed event from the queue.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_spinner_change_queue() -> Option<usize> {
    unsafe {
        if SPIN_CHANGE_QUEUE_TAIL == SPIN_CHANGE_QUEUE_HEAD {
            return None;
        }
        let handle = SPIN_CHANGE_QUEUE[SPIN_CHANGE_QUEUE_TAIL];
        SPIN_CHANGE_QUEUE_TAIL = (SPIN_CHANGE_QUEUE_TAIL + 1) % SPIN_CHANGE_QUEUE_SIZE;
        Some(handle)
    }
}

/// Look up the Java object heap index for a spinner given its raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_spinner_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &SPIN_HANDLE_MAP[..SPIN_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Reset all spinner state between app runs.
pub fn reset_spinner_state() {
    unsafe {
        SPIN_HANDLE_MAP_LEN = 0;
        SPIN_CHANGE_QUEUE_HEAD = 0;
        SPIN_CHANGE_QUEUE_TAIL = 0;
    }
}
