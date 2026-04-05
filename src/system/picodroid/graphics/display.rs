//! Native method implementations for `picodroid.graphics.Display`.

use crate::hal;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::engine;
use super::fields;
use super::view;
use crate::lvgl_ffi::*;

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

use core::sync::atomic::{AtomicU16, Ordering};

/// Heap index of the singleton `Display` object (`u16::MAX` = not yet allocated).
static DISPLAY_INSTANCE: AtomicU16 = AtomicU16::new(u16::MAX);

/// `Display.getInstance()` — initialises the display hardware + LVGL on first call.
pub fn get_instance(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    let existing = DISPLAY_INSTANCE.load(Ordering::Relaxed);
    if existing != u16::MAX && objects.is_live(existing) {
        return Ok(Some(Value::ObjectRef(existing)));
    }

    // First call — bring up the hardware + LVGL engine (idempotent).
    engine::init();

    let idx = objects
        .alloc("picodroid/graphics/Display")
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(
            idx,
            fields::display::WIDTH,
            Value::Int(hal::display::WIDTH as i32),
        )
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(
            idx,
            fields::display::HEIGHT,
            Value::Int(hal::display::HEIGHT as i32),
        )
        .ok_or(JvmError::StackOverflow)?;

    DISPLAY_INSTANCE.store(idx, Ordering::Relaxed);
    Ok(Some(Value::ObjectRef(idx)))
}

// ---------------------------------------------------------------------------
// Content management
// ---------------------------------------------------------------------------

/// `Display.setContentView(View root)` — clears the screen and parents `root` to it.
pub fn set_content_view(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let root_handle = view::extract_handle_at(args, 1, objects)?;
    unsafe {
        let screen = engine::screen();
        lv_obj_clean(screen);
        lv_obj_set_parent(root_handle as *mut lv_obj_t, screen);
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Touch
// ---------------------------------------------------------------------------

/// `Display.pollTouch()` — returns a `MotionEvent` or `null`.
pub fn poll_touch(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    match hal::touch::read_point() {
        Some((x, y)) => {
            let idx = objects
                .alloc("picodroid/view/MotionEvent")
                .ok_or(JvmError::StackOverflow)?;
            objects
                .set_field(idx, fields::motion_event::ACTION, Value::Int(0))
                .ok_or(JvmError::StackOverflow)?; // ACTION_DOWN
            objects
                .set_field(idx, fields::motion_event::X, Value::Int(x as i32))
                .ok_or(JvmError::StackOverflow)?;
            objects
                .set_field(idx, fields::motion_event::Y, Value::Int(y as i32))
                .ok_or(JvmError::StackOverflow)?;
            Ok(Some(Value::ObjectRef(idx)))
        }
        None => Ok(Some(Value::Null)),
    }
}

// ---------------------------------------------------------------------------
// Tick
// ---------------------------------------------------------------------------

/// `Display.update()` — advances the LVGL timer and renders dirty regions.
pub fn update() -> Result<Option<Value>, JvmError> {
    engine::tick(16);
    Ok(None)
}
