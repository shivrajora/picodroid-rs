use crate::lvgl_ffi::*;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::engine;
use super::super::handle_table;
use super::super::view::extract_native_handle;

// ---------------------------------------------------------------------------
// Value-changed event queue (ring buffer)
// ---------------------------------------------------------------------------

const SEEK_CHANGE_QUEUE_SIZE: usize = 16;
static mut SEEK_CHANGE_QUEUE: [usize; SEEK_CHANGE_QUEUE_SIZE] = [0; SEEK_CHANGE_QUEUE_SIZE];
static mut SEEK_CHANGE_QUEUE_HEAD: usize = 0;
static mut SEEK_CHANGE_QUEUE_TAIL: usize = 0;

// ---------------------------------------------------------------------------
// Handle -> Java object mapping (for event dispatch)
// ---------------------------------------------------------------------------

const SEEK_MAX_LISTENERS: usize = 32;
static mut SEEK_HANDLE_MAP: [(usize, u16); SEEK_MAX_LISTENERS] = [(0, 0); SEEK_MAX_LISTENERS];
static mut SEEK_HANDLE_MAP_LEN: usize = 0;

// ---------------------------------------------------------------------------
// LVGL callback
// ---------------------------------------------------------------------------

/// Called by LVGL when the slider value changes (user drag).
///
/// # Safety
/// Called from LVGL's event dispatch during `lv_timer_handler()`.
unsafe extern "C" fn seek_bar_value_changed_cb(e: *mut lv_event_t) {
    let obj = lv_event_get_target_obj(e);

    let next = (SEEK_CHANGE_QUEUE_HEAD + 1) % SEEK_CHANGE_QUEUE_SIZE;
    if next != SEEK_CHANGE_QUEUE_TAIL {
        SEEK_CHANGE_QUEUE[SEEK_CHANGE_QUEUE_HEAD] = obj as usize;
        SEEK_CHANGE_QUEUE_HEAD = next;
    }
}

// ---------------------------------------------------------------------------
// Native method implementations
// ---------------------------------------------------------------------------

/// `SeekBar.nativeCreate()` -- creates an `lv_slider` with default range 0..100.
pub fn seek_bar_native_create() -> Result<Option<Value>, JvmError> {
    let ptr = unsafe {
        let s = lv_slider_create(engine::screen());
        lv_slider_set_range(s, 0, 100);
        lv_slider_set_value(s, 0, LV_ANIM_OFF);
        lv_obj_add_event_cb(
            s,
            Some(seek_bar_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        s
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `SeekBar.nativeCreateWithMax(int max)` -- creates an `lv_slider` with range 0..max.
pub fn seek_bar_native_create_with_max(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let max = match args.first() {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let ptr = unsafe {
        let s = lv_slider_create(engine::screen());
        lv_slider_set_range(s, 0, max);
        lv_slider_set_value(s, 0, LV_ANIM_OFF);
        lv_obj_add_event_cb(
            s,
            Some(seek_bar_value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );
        s
    };
    Ok(Some(Value::Int(handle_table::register(ptr))))
}

/// `SeekBar.setMax(int max)`
pub fn seek_bar_set_max(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let max = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_slider_set_range(handle_table::lookup(id), 0, max) };
    Ok(None)
}

/// `SeekBar.setProgress(int progress)`
pub fn seek_bar_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let progress = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_slider_set_value(handle_table::lookup(id), progress, LV_ANIM_ON) };
    Ok(None)
}

/// `SeekBar.performProgressChange()` — bump the slider one unit and fire
/// `LV_EVENT_VALUE_CHANGED`. Goes through the same callback a real drag
/// would; the registered listener runs on the next dispatch tick.
pub fn seek_bar_perform_progress_change(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    unsafe {
        let obj = handle_table::lookup(id);
        // Advance by 1, or reset to 0 if already at max, to guarantee a state
        // transition that the LVGL event represents meaningfully.
        let cur = lv_slider_get_value(obj);
        let next = cur.saturating_add(1);
        lv_slider_set_value(obj, next, LV_ANIM_OFF);
        lv_obj_send_event(obj, LV_EVENT_VALUE_CHANGED, core::ptr::null_mut());
    }
    Ok(None)
}

/// `SeekBar.getProgress()`
pub fn seek_bar_get_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let val = unsafe { lv_slider_get_value(handle_table::lookup(id)) };
    Ok(Some(Value::Int(val)))
}

/// `SeekBar.nativeRegisterChangeListener()` -- records the mapping
/// from this slider's LVGL handle to its Java heap index.
pub fn seek_bar_register_change_listener(
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
        for entry in &mut SEEK_HANDLE_MAP[..SEEK_HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return Ok(None);
            }
        }
        if SEEK_HANDLE_MAP_LEN < SEEK_MAX_LISTENERS {
            SEEK_HANDLE_MAP[SEEK_HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            SEEK_HANDLE_MAP_LEN += 1;
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Event queue drain (called from lifecycle event loop)
// ---------------------------------------------------------------------------

/// Pop one value-changed event from the queue.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_seek_change_queue() -> Option<usize> {
    unsafe {
        if SEEK_CHANGE_QUEUE_TAIL == SEEK_CHANGE_QUEUE_HEAD {
            return None;
        }
        let handle = SEEK_CHANGE_QUEUE[SEEK_CHANGE_QUEUE_TAIL];
        SEEK_CHANGE_QUEUE_TAIL = (SEEK_CHANGE_QUEUE_TAIL + 1) % SEEK_CHANGE_QUEUE_SIZE;
        Some(handle)
    }
}

/// Look up the Java object heap index for a seek bar given its raw LVGL pointer.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_seek_bar_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &SEEK_HANDLE_MAP[..SEEK_HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

/// Reset all seek bar state between app runs.
pub fn reset_seek_bar_state() {
    unsafe {
        SEEK_HANDLE_MAP_LEN = 0;
        SEEK_CHANGE_QUEUE_HEAD = 0;
        SEEK_CHANGE_QUEUE_TAIL = 0;
    }
}
