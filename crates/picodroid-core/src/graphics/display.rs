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
    // A new root starts at the window's origin: the pan the previous root
    // left on the screen is not its own.
    super::lvgl::window::reset_pan();
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
/// root's tree fits the window, how far the screen pans to show the root,
/// or how far the tree reaches past the window's edge inside a layout
/// that does not scroll. A resizeable app is meant to fit every board;
/// the nightly screen matrix asserts `fit ok` on each geometry. Sim and
/// debug builds only — release firmware has no reader for it.
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
    let (w, h, pan_x, pan_y, cut_x, cut_y, by, short_x, short_y) = unsafe {
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
        let mut win = a;
        lv_obj_get_coords(scr, &mut win);
        // Past the window's edge inside the tree, and past the edge of a
        // layout inside it. The window's own scroll extent sees only its
        // direct child, the root; a `match_parent` root never overflows —
        // its children do, inside it, cut by the edge with nothing to pan
        // to (QA F3). A layout smaller than the window cuts its children at
        // its own edge the same way, short of the window's (round 2, B2).
        let r = reach(root, 0);
        let win_cut_x = (r.right - win.x2).max(0);
        let win_cut_y = (r.bottom - win.y2).max(0);
        // Name the layout when it cuts more than the window does; a
        // `match_parent` root's edge is the window's, so the window form
        // stands for it.
        let by = (r.clip_x > win_cut_x || r.clip_y > win_cut_y).then_some(r.by);
        (
            a.x2 - a.x1 + 1,
            a.y2 - a.y1 + 1,
            lv_obj_get_scroll_right(scr).max(0),
            lv_obj_get_scroll_bottom(scr).max(0),
            win_cut_x.max(r.clip_x),
            win_cut_y.max(r.clip_y),
            by,
            (win.x2 - a.x2).max(0),
            (win.y2 - a.y2).max(0),
        )
    };
    if pan_x == 0 && pan_y == 0 && cut_x == 0 && cut_y == 0 {
        if short_x > 0 || short_y > 0 {
            // A root an app sized itself, smaller than the window (B1's
            // symptom before a content root defaulted to the window): the
            // tree fits, and part of the panel is blank.
            crate::pd_info!(
                "[layout] fit ok {}x{} in {}x{} (the root stops {} short of the right edge, {} short of the bottom)",
                w,
                h,
                win_w,
                win_h,
                short_x,
                short_y
            );
        } else {
            crate::pd_info!("[layout] fit ok {}x{} in {}x{}", w, h, win_w, win_h);
        }
    } else if pan_x > 0 || pan_y > 0 {
        crate::pd_info!(
            "[layout] overflow {}x{} in {}x{}: the screen pans {} right, {} down",
            w,
            h,
            win_w,
            win_h,
            pan_x.max(cut_x),
            pan_y.max(cut_y)
        );
    } else if let Some((bw, bh)) = by {
        crate::pd_info!(
            "[layout] overflow {}x{} in {}x{}: content is cut {} past the right edge, {} past the bottom (by a {}x{} layout)",
            w,
            h,
            win_w,
            win_h,
            cut_x,
            cut_y,
            bw,
            bh
        );
    } else {
        crate::pd_info!(
            "[layout] overflow {}x{} in {}x{}: content is cut {} past the right edge, {} past the bottom",
            w,
            h,
            win_w,
            win_h,
            cut_x,
            cut_y
        );
    }
}

/// What [`reach`] found in a view tree: the farthest right and bottom
/// edge on show, and the most a non-scrolling layout in the tree cuts off
/// its own children, per axis, with the size of the layout that cuts the
/// most.
#[cfg(any(feature = "sim", debug_assertions))]
#[derive(Clone, Copy)]
struct Reach {
    right: i32,
    bottom: i32,
    clip_x: i32,
    clip_y: i32,
    /// Width and height of the layout behind the larger of the two clips.
    by: (i32, i32),
}

#[cfg(any(feature = "sim", debug_assertions))]
impl Reach {
    /// Fold in a cut of `x` by `y` (either may be negative: nothing cut)
    /// made by a layout of size `by`.
    fn cut(&mut self, x: i32, y: i32, by: (i32, i32)) {
        let (x, y) = (x.max(0), y.max(0));
        if x.max(y) > self.clip_x.max(self.clip_y) {
            self.by = by;
        }
        self.clip_x = self.clip_x.max(x);
        self.clip_y = self.clip_y.max(y);
    }
}

/// The farthest right and bottom edge in `obj`'s tree that is on show
/// without scrolling, and what the tree's layouts cut: `obj`'s own edge,
/// then through every child that is shown and laid out by its parent,
/// stopping at a scroll container, whose content is reached by scrolling
/// it (a `ScrollView`, a list) — a picodroid layout does not scroll, so
/// what it holds past its edge is cut, at that edge, whether or not the
/// window's is further out. Bounded in depth; a view tree is a dozen
/// levels at most.
///
/// # Safety
/// `obj` must be a live LVGL object, read on the UI task.
#[cfg(any(feature = "sim", debug_assertions))]
unsafe fn reach(obj: *mut crate::lvgl_ffi::lv_obj_t, depth: u32) -> Reach {
    use crate::lvgl_ffi::{
        lv_area_t, lv_obj_get_child, lv_obj_get_child_count, lv_obj_get_coords, lv_obj_has_flag,
        LV_OBJ_FLAG_FLOATING, LV_OBJ_FLAG_HIDDEN, LV_OBJ_FLAG_SCROLLABLE,
    };
    const MAX_DEPTH: u32 = 16;
    let mut a = lv_area_t {
        x1: 0,
        y1: 0,
        x2: 0,
        y2: 0,
    };
    // SAFETY: `obj` is live (the caller's contract; a child LVGL just
    // listed is live too), and these are getters on the UI task.
    unsafe {
        lv_obj_get_coords(obj, &mut a);
        let mut r = Reach {
            right: a.x2,
            bottom: a.y2,
            clip_x: 0,
            clip_y: 0,
            by: (0, 0),
        };
        if depth >= MAX_DEPTH || lv_obj_has_flag(obj, LV_OBJ_FLAG_SCROLLABLE) {
            return r;
        }
        let size = (a.x2 - a.x1 + 1, a.y2 - a.y1 + 1);
        for i in 0..lv_obj_get_child_count(obj) {
            let child = lv_obj_get_child(obj, i as i32);
            if child.is_null()
                || lv_obj_has_flag(child, LV_OBJ_FLAG_HIDDEN)
                || lv_obj_has_flag(child, LV_OBJ_FLAG_FLOATING)
            {
                continue;
            }
            let c = reach(child, depth + 1);
            r.right = r.right.max(c.right);
            r.bottom = r.bottom.max(c.bottom);
            // What `obj` cuts off this child at its own edge, then what the
            // child's layouts cut inside it.
            r.cut(c.right - a.x2, c.bottom - a.y2, size);
            r.cut(c.clip_x, c.clip_y, c.by);
        }
        r
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
