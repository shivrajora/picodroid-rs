//! Native method implementations for `picodroid.graphics.Display`.

use crate::hal;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::gfx::Handle;
use super::lvgl::{calibration, with_gfx};
use super::view;

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

use core::sync::atomic::{AtomicU16, Ordering};

/// Heap index of the singleton `Display` object (`u16::MAX` = not yet allocated).
static DISPLAY_INSTANCE: AtomicU16 = AtomicU16::new(u16::MAX);

/// Java `nativeHandle` of the current root view installed by
/// `setContentView`. `0` = no root set yet. Single-threaded access (the
/// JVM owns the only frontend), same contract as the prior `usize` cell.
static mut CURRENT_ROOT_ID: i32 = 0;

/// `Display.getInstance()` — initialises the display hardware + LVGL on first call.
pub fn get_instance(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    let existing = DISPLAY_INSTANCE.load(Ordering::Relaxed);
    if existing != u16::MAX && objects.is_live(existing) {
        return Ok(Some(Value::ObjectRef(existing)));
    }

    // First call — bring up the hardware + LVGL engine. `LvglGfx::init` is
    // idempotent so this is safe across PDB hot-reloads.
    with_gfx(|g| g.init(hal::display::WIDTH, hal::display::HEIGHT));

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

/// `Display.setContentView(View root)` — installs `root` as the screen's
/// content view, deleting the previous root if different.
pub fn set_content_view(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let root_id = view::extract_handle_at(args, 1, objects)?;
    // SAFETY: single-threaded access matches the prior usize-cell contract.
    let prev_id = unsafe {
        let prev = CURRENT_ROOT_ID;
        CURRENT_ROOT_ID = root_id;
        prev
    };

    with_gfx(|g| {
        if prev_id != 0 && prev_id != root_id {
            g.delete(Handle::from_java(prev_id));
        }
        // Ensure the new root is parented to the screen (every nativeCreate
        // already parents to the active screen on first call, so this is a
        // re-parent on subsequent setContentView calls).
        let scr = g.screen();
        g.set_parent(Handle::from_java(root_id), scr);
    });
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

/// `Display.calibrate()` — runs interactive 4-point touch calibration.
pub fn calibrate() -> Result<Option<Value>, JvmError> {
    calibration::calibrate();
    Ok(None)
}

/// `Display.showFps()` — enables the on-screen FPS overlay.
pub fn show_fps() -> Result<Option<Value>, JvmError> {
    super::lvgl::fps_overlay::enable();
    Ok(None)
}

/// `Display.update()` — advances the LVGL timer and renders dirty regions.
pub fn update() -> Result<Option<Value>, JvmError> {
    with_gfx(|g| g.tick(16));
    Ok(None)
}
