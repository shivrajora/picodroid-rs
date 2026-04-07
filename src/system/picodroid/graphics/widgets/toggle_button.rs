use core::ffi::c_char;

use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::{extract_native_handle, java_str_to_cstr};

// ---------------------------------------------------------------------------
// Per-toggle-button text storage
// ---------------------------------------------------------------------------

const MAX_TOGGLE_BUTTONS: usize = 16;
const TEXT_BUF_SIZE: usize = 32;

struct ToggleButtonEntry {
    /// Raw `lv_obj_t*` cast to `usize` — matches the value from LVGL callbacks.
    raw_ptr: usize,
    text_on: [u8; TEXT_BUF_SIZE],
    text_off: [u8; TEXT_BUF_SIZE],
}

impl ToggleButtonEntry {
    const fn empty() -> Self {
        Self {
            raw_ptr: 0,
            text_on: [0; TEXT_BUF_SIZE],
            text_off: [0; TEXT_BUF_SIZE],
        }
    }
}

const EMPTY_ENTRY: ToggleButtonEntry = ToggleButtonEntry::empty();

static mut TOGGLE_BUTTONS: [ToggleButtonEntry; MAX_TOGGLE_BUTTONS] =
    [EMPTY_ENTRY; MAX_TOGGLE_BUTTONS];
static mut TOGGLE_BUTTON_COUNT: usize = 0;

// ---------------------------------------------------------------------------
// Checked-change event queue (ring buffer)
// ---------------------------------------------------------------------------

const CHECKED_CHANGE_QUEUE_SIZE: usize = 16;
/// Stores raw `lv_obj_t*` values from LVGL callbacks.
static mut CHECKED_CHANGE_QUEUE: [usize; CHECKED_CHANGE_QUEUE_SIZE] =
    [0; CHECKED_CHANGE_QUEUE_SIZE];
static mut CHECKED_CHANGE_QUEUE_HEAD: usize = 0;
static mut CHECKED_CHANGE_QUEUE_TAIL: usize = 0;

// ---------------------------------------------------------------------------
// Handle → Java object mapping (for event dispatch)
// ---------------------------------------------------------------------------

const MAX_LISTENERS: usize = 32;
/// Maps raw `lv_obj_t*` to Java heap object indices.
static mut CHECKED_CHANGE_HANDLE_MAP: [(usize, u16); MAX_LISTENERS] = [(0, 0); MAX_LISTENERS];
static mut CHECKED_CHANGE_HANDLE_MAP_LEN: usize = 0;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Copy a null-terminated C string into a fixed-size buffer, truncating if needed.
#[allow(clippy::unnecessary_cast)]
fn copy_cstr_to_buf(src: *const c_char, dst: &mut [u8; TEXT_BUF_SIZE]) {
    let mut i = 0;
    unsafe {
        while i < TEXT_BUF_SIZE - 1 {
            let b = *src.add(i) as u8;
            if b == 0 {
                break;
            }
            dst[i] = b;
            i += 1;
        }
    }
    dst[i] = 0;
}

/// Find the entry for a given raw `lv_obj_t*` pointer (stored as `usize`).
unsafe fn find_entry(raw_ptr: usize) -> Option<&'static mut ToggleButtonEntry> {
    TOGGLE_BUTTONS[..TOGGLE_BUTTON_COUNT]
        .iter_mut()
        .find(|entry| entry.raw_ptr == raw_ptr)
}

/// Update the LVGL label to match the current checked state.
unsafe fn update_label(obj: *mut lv_obj_t) {
    if let Some(entry) = find_entry(obj as usize) {
        let label = lv_obj_get_child(obj, 0);
        if !label.is_null() {
            let checked = lv_obj_has_state(obj, LV_STATE_CHECKED);
            let text = if checked {
                &entry.text_on
            } else {
                &entry.text_off
            };
            lv_label_set_text(label, text.as_ptr() as *const c_char);
        }
    }
}

/// Register a new toggle button entry with default or provided text.
unsafe fn register_entry(raw_ptr: usize, text_on: &[u8], text_off: &[u8]) {
    if TOGGLE_BUTTON_COUNT >= MAX_TOGGLE_BUTTONS {
        return;
    }
    let entry = &mut TOGGLE_BUTTONS[TOGGLE_BUTTON_COUNT];
    entry.raw_ptr = raw_ptr;
    let on_len = text_on.len().min(TEXT_BUF_SIZE - 1);
    entry.text_on[..on_len].copy_from_slice(&text_on[..on_len]);
    entry.text_on[on_len] = 0;
    let off_len = text_off.len().min(TEXT_BUF_SIZE - 1);
    entry.text_off[..off_len].copy_from_slice(&text_off[..off_len]);
    entry.text_off[off_len] = 0;
    TOGGLE_BUTTON_COUNT += 1;
}

// ---------------------------------------------------------------------------
// LVGL callback
// ---------------------------------------------------------------------------

/// Called by LVGL when the toggle button's checked state changes (user tap).
///
/// # Safety
/// Called from LVGL's event dispatch during `lv_timer_handler()`.
unsafe extern "C" fn toggle_button_value_changed_cb(e: *mut lv_event_t) {
    let obj = lv_event_get_target_obj(e);

    // Update label text immediately using the raw pointer.
    update_label(obj);

    // Enqueue raw pointer for Java dispatch.
    let next = (CHECKED_CHANGE_QUEUE_HEAD + 1) % CHECKED_CHANGE_QUEUE_SIZE;
    if next != CHECKED_CHANGE_QUEUE_TAIL {
        CHECKED_CHANGE_QUEUE[CHECKED_CHANGE_QUEUE_HEAD] = obj as usize;
        CHECKED_CHANGE_QUEUE_HEAD = next;
    }
}

// ---------------------------------------------------------------------------
// Native method implementations
// ---------------------------------------------------------------------------

/// `ToggleButton.nativeCreate()` — creates a checkable button with "ON"/"OFF" defaults.
pub fn toggle_button_native_create() -> Result<Option<Value>, JvmError> {
    let screen = engine::screen();
    let btn = unsafe { lv_button_create(screen) };

    // Make the button checkable so LVGL auto-toggles LV_STATE_CHECKED.
    unsafe { lv_obj_add_flag(btn, LV_OBJ_FLAG_CHECKABLE) };

    // Create child label showing the initial (unchecked) text.
    let label = unsafe { lv_label_create(btn) };
    unsafe { lv_label_set_text(label, c"OFF".as_ptr()) };
    unsafe { lv_obj_center(label) };

    // Register value-changed callback.
    unsafe {
        lv_obj_add_event_cb(
            btn,
            Some(toggle_button_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
    }

    // Store entry with default text, keyed by raw pointer.
    unsafe { register_entry(btn as usize, b"ON", b"OFF") };

    Ok(Some(Value::Int(handle_table::register(btn))))
}

/// `ToggleButton.nativeCreateWithText(String textOn, String textOff)`
pub fn toggle_button_native_create_with_text(
    args: &[Value],
    strings: &StringTable,
) -> Result<Option<Value>, JvmError> {
    let screen = engine::screen();
    let btn = unsafe { lv_button_create(screen) };
    unsafe { lv_obj_add_flag(btn, LV_OBJ_FLAG_CHECKABLE) };

    // Parse textOn and textOff from args.
    let mut on_buf = [0u8; 128];
    let mut off_buf = [0u8; 128];
    let text_on_cstr = args
        .first()
        .ok_or(JvmError::InvalidReference)
        .and_then(|v| java_str_to_cstr(v, strings, &mut on_buf))?;
    let text_off_cstr = args
        .get(1)
        .ok_or(JvmError::InvalidReference)
        .and_then(|v| java_str_to_cstr(v, strings, &mut off_buf))?;

    // Create child label with the initial (unchecked) text.
    let label = unsafe { lv_label_create(btn) };
    unsafe { lv_label_set_text(label, text_off_cstr) };
    unsafe { lv_obj_center(label) };

    unsafe {
        lv_obj_add_event_cb(
            btn,
            Some(toggle_button_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
    }

    // Store entry with provided text (copy from the C strings), keyed by raw pointer.
    unsafe {
        if TOGGLE_BUTTON_COUNT < MAX_TOGGLE_BUTTONS {
            let entry = &mut TOGGLE_BUTTONS[TOGGLE_BUTTON_COUNT];
            entry.raw_ptr = btn as usize;
            copy_cstr_to_buf(text_on_cstr, &mut entry.text_on);
            copy_cstr_to_buf(text_off_cstr, &mut entry.text_off);
            TOGGLE_BUTTON_COUNT += 1;
        }
    }

    Ok(Some(Value::Int(handle_table::register(btn))))
}

/// `ToggleButton.isChecked()`
pub fn toggle_button_is_checked(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let checked = unsafe { lv_obj_has_state(handle_table::lookup(id), LV_STATE_CHECKED) };
    Ok(Some(Value::Int(if checked { 1 } else { 0 })))
}

/// `ToggleButton.setChecked(boolean checked)`
pub fn toggle_button_set_checked(
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
        update_label(obj);
    }
    Ok(None)
}

/// `ToggleButton.toggle()`
pub fn toggle_button_toggle(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    unsafe {
        let obj = handle_table::lookup(id);
        let checked = lv_obj_has_state(obj, LV_STATE_CHECKED);
        if checked {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        }
        update_label(obj);
    }
    Ok(None)
}

/// `ToggleButton.setTextOn(String text)`
pub fn toggle_button_set_text_on(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe {
        let obj = handle_table::lookup(id);
        if let Some(entry) = find_entry(obj as usize) {
            copy_cstr_to_buf(cstr, &mut entry.text_on);
        }
        // If currently checked, update the displayed label too.
        if lv_obj_has_state(obj, LV_STATE_CHECKED) {
            let label = lv_obj_get_child(obj, 0);
            if !label.is_null() {
                lv_label_set_text(label, cstr);
            }
        }
    }
    Ok(None)
}

/// `ToggleButton.setTextOff(String text)`
pub fn toggle_button_set_text_off(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe {
        let obj = handle_table::lookup(id);
        if let Some(entry) = find_entry(obj as usize) {
            copy_cstr_to_buf(cstr, &mut entry.text_off);
        }
        // If currently unchecked, update the displayed label too.
        if !lv_obj_has_state(obj, LV_STATE_CHECKED) {
            let label = lv_obj_get_child(obj, 0);
            if !label.is_null() {
                lv_label_set_text(label, cstr);
            }
        }
    }
    Ok(None)
}

/// `ToggleButton.nativeRegisterCheckedChangeListener()` — records the mapping
/// from this toggle button's LVGL handle to its Java heap index.
pub fn toggle_button_register_checked_change_listener(
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
        for entry in &mut CHECKED_CHANGE_HANDLE_MAP[..CHECKED_CHANGE_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        // New registration.
        if CHECKED_CHANGE_HANDLE_MAP_LEN < MAX_LISTENERS {
            CHECKED_CHANGE_HANDLE_MAP[CHECKED_CHANGE_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            CHECKED_CHANGE_HANDLE_MAP_LEN += 1;
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Event queue drain (called from lifecycle event loop)
// ---------------------------------------------------------------------------

/// Pop one checked-change event from the queue, returning the raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_checked_change_queue() -> Option<usize> {
    unsafe {
        if CHECKED_CHANGE_QUEUE_TAIL == CHECKED_CHANGE_QUEUE_HEAD {
            return None;
        }
        let handle = CHECKED_CHANGE_QUEUE[CHECKED_CHANGE_QUEUE_TAIL];
        CHECKED_CHANGE_QUEUE_TAIL = (CHECKED_CHANGE_QUEUE_TAIL + 1) % CHECKED_CHANGE_QUEUE_SIZE;
        Some(handle)
    }
}

/// Look up the Java object heap index for a toggle button given its raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_checked_change_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &CHECKED_CHANGE_HANDLE_MAP[..CHECKED_CHANGE_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Reset all toggle button state between app runs.
pub fn reset_toggle_button_state() {
    unsafe {
        TOGGLE_BUTTON_COUNT = 0;
        CHECKED_CHANGE_HANDLE_MAP_LEN = 0;
        CHECKED_CHANGE_QUEUE_HEAD = 0;
        CHECKED_CHANGE_QUEUE_TAIL = 0;
    }
}
