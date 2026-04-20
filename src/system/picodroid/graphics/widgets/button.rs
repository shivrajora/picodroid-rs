use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_native_handle, java_str_to_cstr};

/// Static ring buffer for button click events.
/// Stores `lv_obj_t*` addresses of clicked buttons; `wasClicked` drains them.
const CLICK_QUEUE_SIZE: usize = 16;
static mut CLICK_QUEUE: [usize; CLICK_QUEUE_SIZE] = [0; CLICK_QUEUE_SIZE];
static mut CLICK_QUEUE_HEAD: usize = 0;
static mut CLICK_QUEUE_TAIL: usize = 0;

// ---------------------------------------------------------------------------
// Button handle → Java object mapping (for Activity click dispatch)
// ---------------------------------------------------------------------------

const MAX_BUTTONS: usize = 32;
/// Maps LVGL `lv_obj_t*` button handles to Java heap object indices.
static mut BUTTON_HANDLE_MAP: [(usize, u16); MAX_BUTTONS] = [(0, 0); MAX_BUTTONS];
static mut BUTTON_HANDLE_MAP_LEN: usize = 0;

/// LVGL event callback registered on every button.
///
/// # Safety
/// Called from LVGL's event dispatch during `lv_timer_handler()`.
unsafe extern "C" fn button_click_cb(e: *mut lv_event_t) {
    let obj = lv_event_get_target_obj(e);
    let head = CLICK_QUEUE_HEAD;
    let next = (head + 1) % CLICK_QUEUE_SIZE;
    if next != CLICK_QUEUE_TAIL {
        CLICK_QUEUE[head] = obj as usize;
        CLICK_QUEUE_HEAD = next;
    }
}

/// `Button.nativeCreate(String text)` — creates `lv_button` + child `lv_label`.
pub fn button_native_create(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let screen = engine::screen();
    let btn = unsafe { lv_button_create(screen) };

    // Create child label
    let label = unsafe { lv_label_create(btn) };
    if let Some(text_arg) = args.first() {
        let mut buf = [0u8; 128];
        if let Ok(cstr) = java_str_to_cstr(text_arg, strings, &mut buf) {
            unsafe { lv_label_set_text(label, cstr) };
        }
    }
    unsafe { lv_obj_center(label) };

    // Register click callback
    unsafe {
        lv_obj_add_event_cb(
            btn,
            Some(button_click_cb),
            LV_EVENT_CLICKED,
            core::ptr::null_mut(),
        );
    }

    Ok(Some(Value::Int(handle_table::register(btn))))
}

/// `Button.setText(String text)`
pub fn button_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe {
        // The first child of a button is its label.
        let label = lv_obj_get_child(handle_table::lookup(id), 0);
        if !label.is_null() {
            lv_label_set_text(label, cstr);
        }
    }
    Ok(None)
}

/// `Button.wasClicked()` — returns `true` if this button was clicked since last poll.
pub fn button_was_clicked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let raw_ptr = handle_table::lookup(id) as usize;
    unsafe {
        let mut i = CLICK_QUEUE_TAIL;
        while i != CLICK_QUEUE_HEAD {
            if CLICK_QUEUE[i] == raw_ptr {
                // Consume this event by compacting the queue: shift remaining entries.
                let mut j = i;
                loop {
                    let next = (j + 1) % CLICK_QUEUE_SIZE;
                    if next == CLICK_QUEUE_HEAD {
                        break;
                    }
                    CLICK_QUEUE[j] = CLICK_QUEUE[next];
                    j = next;
                }
                CLICK_QUEUE_HEAD = if CLICK_QUEUE_HEAD == 0 {
                    CLICK_QUEUE_SIZE - 1
                } else {
                    CLICK_QUEUE_HEAD - 1
                };
                return Ok(Some(Value::Int(1))); // true
            }
            i = (i + 1) % CLICK_QUEUE_SIZE;
        }
    }
    Ok(Some(Value::Int(0))) // false
}

/// `Button.performClick()` — synthetically fire `LV_EVENT_CLICKED` on the
/// underlying LVGL button. The existing `button_click_cb` handles it
/// identically to a real touch, so the registered `OnClickListener` runs
/// on the next dispatch tick. Android parity; also the entry point
/// `examples/callbacktest` uses to exercise the post-`--shrink` dispatch
/// path without a human clicking.
pub fn button_perform_click(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    unsafe {
        let obj = handle_table::lookup(id);
        lv_obj_send_event(obj, LV_EVENT_CLICKED, core::ptr::null_mut());
    }
    Ok(None)
}

/// `Button.nativeRegisterClickListener()` — records the mapping from this
/// button's LVGL handle to its Java heap index so the framework event loop
/// can dispatch `fireClick()` on the correct object.
pub fn button_register_click_listener(
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
        for entry in &mut BUTTON_HANDLE_MAP[..BUTTON_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        // New registration.
        if BUTTON_HANDLE_MAP_LEN < MAX_BUTTONS {
            BUTTON_HANDLE_MAP[BUTTON_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            BUTTON_HANDLE_MAP_LEN += 1;
        }
    }
    Ok(None)
}

/// Look up the Java object heap index for a button given its LVGL handle.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_button_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &BUTTON_HANDLE_MAP[..BUTTON_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Pop one click event from the queue, returning the LVGL handle.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_click_queue() -> Option<usize> {
    unsafe {
        if CLICK_QUEUE_TAIL == CLICK_QUEUE_HEAD {
            return None;
        }
        let handle = CLICK_QUEUE[CLICK_QUEUE_TAIL];
        CLICK_QUEUE_TAIL = (CLICK_QUEUE_TAIL + 1) % CLICK_QUEUE_SIZE;
        Some(handle)
    }
}

/// Reset all button state between app runs.
pub fn reset_button_state() {
    unsafe {
        BUTTON_HANDLE_MAP_LEN = 0;
        CLICK_QUEUE_HEAD = 0;
        CLICK_QUEUE_TAIL = 0;
    }
}
