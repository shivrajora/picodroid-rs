//! Native method implementations for all widget classes:
//! `TextView`, `Button`, `LinearLayout`, `ProgressBar`, `Switch`, `ListView`, `ImageView`.

use crate::lvgl_ffi::*;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::engine;
use super::view::{extract_native_handle, java_str_to_cstr};

// ===========================================================================
// TextView
// ===========================================================================

/// `TextView.nativeCreate()` — creates an `lv_label` on the active screen.
pub fn text_view_native_create() -> Result<Option<Value>, JvmError> {
    let handle = unsafe { lv_label_create(engine::screen()) };
    Ok(Some(Value::Int(handle as i32)))
}

/// `TextView.setText(String text)`
pub fn text_view_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe { lv_label_set_text(handle as *mut lv_obj_t, cstr) };
    Ok(None)
}

/// `TextView.setTextColor(int argb)`
pub fn text_view_set_text_color(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let color = lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    };
    unsafe { lv_obj_set_style_text_color(handle as *mut lv_obj_t, color, 0) };
    Ok(None)
}

// ===========================================================================
// Button
// ===========================================================================

/// Static ring buffer for button click events.
/// Stores `lv_obj_t*` addresses of clicked buttons; `wasClicked` drains them.
const CLICK_QUEUE_SIZE: usize = 16;
static mut CLICK_QUEUE: [usize; CLICK_QUEUE_SIZE] = [0; CLICK_QUEUE_SIZE];
static mut CLICK_QUEUE_HEAD: usize = 0;
static mut CLICK_QUEUE_TAIL: usize = 0;

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

    Ok(Some(Value::Int(btn as i32)))
}

/// `Button.setText(String text)`
pub fn button_set_text(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe {
        // The first child of a button is its label.
        let label = lv_obj_get_child(handle as *mut lv_obj_t, 0);
        if !label.is_null() {
            lv_label_set_text(label, cstr);
        }
    }
    Ok(None)
}

/// `Button.wasClicked()` — returns `true` if this button was clicked since last poll.
pub fn button_was_clicked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)? as usize;
    unsafe {
        let mut i = CLICK_QUEUE_TAIL;
        while i != CLICK_QUEUE_HEAD {
            if CLICK_QUEUE[i] == handle {
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

// ===========================================================================
// LinearLayout
// ===========================================================================

/// `LinearLayout.nativeCreate()` — creates an `lv_obj` with flex column layout.
pub fn linear_layout_native_create() -> Result<Option<Value>, JvmError> {
    let obj = unsafe {
        let o = lv_obj_create(engine::screen());
        lv_obj_set_flex_flow(o, LV_FLEX_FLOW_COLUMN);
        lv_obj_set_flex_align(
            o,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
            LV_FLEX_ALIGN_CENTER,
        );
        o
    };
    Ok(Some(Value::Int(obj as i32)))
}

/// `LinearLayout.addView(View child)`
pub fn linear_layout_add_view(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let parent_handle = extract_native_handle(args, objects)?;
    let child_handle = super::view::extract_handle_at(args, 1, objects)?;
    unsafe {
        lv_obj_set_parent(
            child_handle as *mut lv_obj_t,
            parent_handle as *mut lv_obj_t,
        );
    }
    Ok(None)
}

/// `LinearLayout.setOrientation(int orientation)`
pub fn linear_layout_set_orientation(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let orientation = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    let flow = if orientation == 0 {
        LV_FLEX_FLOW_ROW
    } else {
        LV_FLEX_FLOW_COLUMN
    };
    unsafe { lv_obj_set_flex_flow(handle as *mut lv_obj_t, flow) };
    Ok(None)
}

// ===========================================================================
// ProgressBar
// ===========================================================================

/// `ProgressBar.nativeCreate()` — creates an `lv_bar`.
pub fn progress_bar_native_create() -> Result<Option<Value>, JvmError> {
    let bar = unsafe {
        let b = lv_bar_create(engine::screen());
        lv_bar_set_value(b, 0, LV_ANIM_OFF);
        b
    };
    Ok(Some(Value::Int(bar as i32)))
}

/// `ProgressBar.setProgress(int value)`
pub fn progress_bar_set_progress(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let value = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe { lv_bar_set_value(handle as *mut lv_obj_t, value, LV_ANIM_ON) };
    Ok(None)
}

// ===========================================================================
// Switch
// ===========================================================================

/// `Switch.nativeCreate()` — creates an `lv_switch`.
pub fn switch_native_create() -> Result<Option<Value>, JvmError> {
    let sw = unsafe { lv_switch_create(engine::screen()) };
    Ok(Some(Value::Int(sw as i32)))
}

/// `Switch.isChecked()`
pub fn switch_is_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let checked = unsafe { lv_obj_has_state(handle as *mut lv_obj_t, LV_STATE_CHECKED) };
    Ok(Some(Value::Int(if checked { 1 } else { 0 })))
}

/// `Switch.setChecked(boolean checked)`
pub fn switch_set_checked(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let checked = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    unsafe {
        let obj = handle as *mut lv_obj_t;
        if checked {
            lv_obj_add_state(obj, LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(obj, LV_STATE_CHECKED);
        }
    }
    Ok(None)
}

// ===========================================================================
// ListView
// ===========================================================================

/// `ListView.nativeCreate()` — creates an `lv_list`.
pub fn list_view_native_create() -> Result<Option<Value>, JvmError> {
    let list = unsafe { lv_list_create(engine::screen()) };
    Ok(Some(Value::Int(list as i32)))
}

/// `ListView.addItem(String text)`
pub fn list_view_add_item(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = extract_native_handle(args, objects)?;
    let text_arg = args.get(1).ok_or(JvmError::InvalidReference)?;
    let mut buf = [0u8; 128];
    let cstr = java_str_to_cstr(text_arg, strings, &mut buf)?;
    unsafe { lv_list_add_text(handle as *mut lv_obj_t, cstr) };
    Ok(None)
}

// ===========================================================================
// ImageView
// ===========================================================================

/// `ImageView.nativeCreate()` — creates an `lv_image`.
pub fn image_view_native_create() -> Result<Option<Value>, JvmError> {
    let img = unsafe { lv_image_create(engine::screen()) };
    Ok(Some(Value::Int(img as i32)))
}

/// `ImageView.setImageSource(String path)` — stub (no filesystem on embedded).
pub fn image_view_set_src(
    _args: &[Value],
    _strings: &StringTable,
    _objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    // Image loading from paths is not supported on embedded targets.
    // This is a placeholder for future built-in image descriptor support.
    Ok(None)
}
