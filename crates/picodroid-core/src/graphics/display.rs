// SPDX-License-Identifier: GPL-3.0-only
//! Native method implementations for `picodroid.graphics.Display`.

use crate::hal;
use crate::util::local::Core0;
use core::cell::Cell;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::gfx::{Handle, Visibility};
use super::lvgl::{calibration, with_gfx};
use super::view;

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

use crate::shrink_names::c;
use core::sync::atomic::{AtomicU16, Ordering};

/// Heap index of the singleton `Display` object (`u16::MAX` = not yet allocated).
static DISPLAY_INSTANCE: AtomicU16 = AtomicU16::new(u16::MAX);

/// Java `nativeHandle` of the current root view installed by
/// `setContentView`. `0` = no root set yet. Single-threaded access (the
/// JVM owns the only frontend), same contract as the prior `usize` cell.
// SAFETY: widget-layer state, reached only from JVM tasks.
static CURRENT_ROOT_ID: Core0<Cell<i32>> = unsafe { Core0::new(Cell::new(0)) };

/// Root the singleton `Display` object during GC so it is never swept.
///
/// `get_instance` caches the Display's heap slot in `DISPLAY_INSTANCE` and hands
/// the same `ObjectRef` back on every call. Nothing on the Java side keeps a
/// field to it, so without this root the GC frees it; its slot is then reused by
/// an unrelated object (e.g. a transient `SensorEvent`), and the next
/// `getInstance()` returns that wrong-class object — every following
/// `Activity.setContentView` (which calls `Display.getInstance().setContentView`)
/// then resolves `setContentView` on the wrong class and throws `NoSuchMethod`.
/// Called from `PicodroidNativeHandler::gc_visit_roots`.
pub fn visit_gc_roots(visit: &mut dyn FnMut(Value)) {
    let existing = DISPLAY_INSTANCE.load(Ordering::Relaxed);
    if existing != u16::MAX {
        visit(Value::ObjectRef(existing));
    }
}

/// `Display.getInstance()` — initialises the display hardware + LVGL on first call.
pub fn get_instance(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    let existing = DISPLAY_INSTANCE.load(Ordering::Relaxed);
    // The slot must still be live AND still be a Display: a missed GC root (or a
    // future regression) could let the slot be swept and reused by another
    // class, in which case we must re-allocate rather than hand back garbage.
    if existing != u16::MAX
        && objects.is_live(existing)
        && objects.class_name(existing) == Some(c::picodroid_graphics_Display)
    {
        return Ok(Some(Value::ObjectRef(existing)));
    }

    // First call — bring up the hardware + LVGL engine. `LvglGfx::init` is
    // idempotent so this is safe across PDB hot-reloads.
    with_gfx(|g| g.init(hal::display::WIDTH, hal::display::HEIGHT));

    // The app's window, not the panel: an app with a design size lays out
    // against that size wherever it runs (graphics/lvgl/window.rs).
    let (width, height) = super::lvgl::window::size();
    let idx = objects
        .alloc(c::picodroid_graphics_Display)
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(idx, fields::display::WIDTH, Value::Int(width as i32))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(idx, fields::display::HEIGHT, Value::Int(height as i32))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(
            idx,
            fields::display::DPI,
            Value::Int(i32::from(crate::board_cfg::display::PHYSICAL_DPI)),
        )
        .ok_or(JvmError::StackOverflow)?;

    DISPLAY_INSTANCE.store(idx, Ordering::Relaxed);
    Ok(Some(Value::ObjectRef(idx)))
}

// ---------------------------------------------------------------------------
// Content management
// ---------------------------------------------------------------------------

/// Read `CURRENT_ROOT_ID`. Used by the lifecycle handler to snapshot the
/// visible root into the current stack entry on push, and to free it on
/// pop.
///
/// `pub` rather than `pub(crate)` only because its caller, `lifecycle.rs`,
/// is still in the platform crate; tighten this back when that moves.
pub fn current_root_id() -> i32 {
    CURRENT_ROOT_ID.get()
}

/// Write `CURRENT_ROOT_ID`. Used by the lifecycle handler to clear it on
/// push (before the new top's `onCreate`/`setContentView`) and to restore
/// the resumed activity's saved root on pop.
///
/// See [`current_root_id`] for why this is `pub`.
pub fn set_current_root_id(id: i32) {
    CURRENT_ROOT_ID.set(id);
}

/// `Display.setContentView(View root)` — installs `root` as the screen's
/// content view, deleting the previous root if different.
pub fn set_content_view(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let root_id = view::extract_handle_at(args, 1, objects)?;
    // SAFETY: single-threaded access matches the prior usize-cell contract.
    let prev_id = {
        let prev = CURRENT_ROOT_ID.get();
        CURRENT_ROOT_ID.set(root_id);
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
        let h = Handle::from_java(root_id);
        let _ = g.set_parent(h, scr);
        // Defensive: if the same root is re-installed after a pause cycle
        // (an app rebuilds in onResume), it must end up visible. Idempotent
        // for the first-time path.
        g.set_visibility(h, Visibility::Visible);
    });
    #[cfg(any(feature = "sim", debug_assertions))]
    FIT_PENDING.set(true);
    Ok(None)
}

/// Set by `setContentView`, cleared by [`fit_check_after_tick`] once LVGL
/// has laid the new root out.
// SAFETY: widget-layer state, reached only from JVM tasks.
#[cfg(any(feature = "sim", debug_assertions))]
static FIT_PENDING: Core0<Cell<bool>> = unsafe { Core0::new(Cell::new(false)) };

/// The `[layout]` line (docs/designs/app-portability-2026-10.md D6): after
/// the first LVGL tick that follows a `setContentView`, say whether the
/// root fits the window or how far the screen now pans to show it. A
/// resizeable app is meant to fit every board; the nightly screen matrix
/// asserts `fit ok` on each geometry. Sim and debug builds only — release
/// firmware has no reader for it.
#[cfg(any(feature = "sim", debug_assertions))]
pub fn fit_check_after_tick() {
    if !FIT_PENDING.replace(false) {
        return;
    }
    let root_id = CURRENT_ROOT_ID.get();
    if root_id == 0 {
        return;
    }
    let root = super::lvgl::handle_table::lookup(root_id);
    if root.is_null() {
        return;
    }
    let (win_w, win_h) = super::lvgl::window::size();
    use crate::lvgl_ffi::{
        lv_area_t, lv_obj_get_coords, lv_obj_get_scroll_bottom, lv_obj_get_scroll_right,
        lv_obj_update_layout,
    };
    // SAFETY: `root` is a live view (the handle table resolved it) and the
    // window object or screen exists while an app runs; these are LVGL
    // getters on the UI task, after the tick that laid the tree out (the
    // explicit update is a no-op then, and the guarantee when it was not).
    let (w, h, over_x, over_y) = unsafe {
        // The app's window (window.rs): the screen, or the design-sized
        // object a `<supports-screens>` app runs in.
        let scr = super::lvgl::window::content_root();
        lv_obj_update_layout(scr);
        let mut a = lv_area_t {
            x1: 0,
            y1: 0,
            x2: 0,
            y2: 0,
        };
        lv_obj_get_coords(root, &mut a);
        (
            a.x2 - a.x1 + 1,
            a.y2 - a.y1 + 1,
            lv_obj_get_scroll_right(scr).max(0),
            lv_obj_get_scroll_bottom(scr).max(0),
        )
    };
    if over_x == 0 && over_y == 0 {
        crate::pd_info!("[layout] fit ok {}x{} in {}x{}", w, h, win_w, win_h);
    } else {
        crate::pd_info!(
            "[layout] overflow {}x{} in {}x{}: the screen pans {} right, {} down",
            w,
            h,
            win_w,
            win_h,
            over_x,
            over_y
        );
    }
}

// ---------------------------------------------------------------------------
// Touch
// ---------------------------------------------------------------------------

/// `Display.pollTouch()` — returns a `MotionEvent` or `null`.
pub fn poll_touch(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    // The sampler's latest reading, not a fresh panel read: on a board where
    // it runs it owns the controller, and a second reader would race the
    // driver's own state.
    match hal::touch_sampler::latest() {
        Some((x, y)) => {
            let idx = objects
                .alloc(c::picodroid_view_MotionEvent)
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
///
/// One fixed period per call, not the measured step the lifecycle tick
/// takes: this is the app pumping frames itself, and its contract is that
/// every call is exactly one frame — `graphicsbench` counts a 2 s animation
/// as `2000 / 16` calls, whatever the work between them costs.
pub fn update() -> Result<Option<Value>, JvmError> {
    with_gfx(|g| g.tick(crate::executors::tick_source::TICK_PERIOD_MS));
    Ok(None)
}
