// SPDX-License-Identifier: GPL-3.0-only
//! LVGL backend — the only [`Gfx`] impl today.
//!
//! Nothing outside `lvgl/` should reference `lv_obj_t` / `lv_event_t` /
//! `lv_color_t`; the rest of the graphics layer goes through [`Gfx`] and
//! opaque [`Handle`]s.

#[cfg_attr(test, allow(unused_imports))]
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg_attr(test, allow(unused_imports))]
use super::gfx::{Gfx, Handle, Reparent, ViewProperty, Visibility};

// Every module here compiles under `cargo test`. They were `cfg(not(test))`
// while the bindings were a module of this crate, whose `extern "C"` block is
// itself `cfg(not(test))` so its drift checks run without LVGL; as the
// `pd-lvgl-sys` crate the bindings are an ordinary dependency of this crate's
// test build, declarations and linked library included. Tests here must still
// call only pure functions -- LVGL is linked, not initialised.
pub mod animations;
pub mod calibration;
pub mod drawable;
pub mod events;
pub mod fps_overlay;
pub mod keep_on;
/// The on-screen BACK/HOME control of a `soft_nav` board; a no-op elsewhere
/// so its callers (the keyboard, dialogs, init) need no cfg of their own.
#[cfg(soft_nav)]
pub mod soft_nav;
#[cfg(not(soft_nav))]
pub mod soft_nav {
    pub fn ensure() {}
    pub fn set_hidden(_hidden: bool) {}
    pub fn raise() {}
}
// The options-menu control: a touch board's way to open the menu (K9); a
// four-key board holds SELECT and draws nothing.
#[cfg(has_touch)]
pub mod menu_button;
#[cfg(not(has_touch))]
pub mod menu_button {
    pub fn ensure() {}
    pub fn set_available(_available: bool) {}
    pub fn set_hidden(_hidden: bool) {}
    pub fn raise() {}
    pub fn reset() {}
}
// Scrolling with the panel's own frame memory, on the boards whose panel can
// (`board_cfg::hw_vscroll`); its arithmetic is host-testable on its own.
#[cfg(hw_vscroll)]
pub mod hw_scroll;
pub mod hw_scroll_math;
pub mod lifecycle;
pub mod style_batch;
pub mod view_ops;
pub mod widgets;
pub mod window;

pub mod edit_mode;
pub mod handle_table;
pub mod key_debounce;
pub mod key_filter;
pub mod key_repeat;
pub mod listener_map;

/// Idempotency guard for [`LvglGfx::init`]. LVGL itself doesn't tolerate
/// `lv_init()` twice; this flag latches on the first successful call so
/// repeated `with_gfx(|g| g.init(...))` from `Display.getInstance` and
/// across PDB app reloads are no-ops.
#[cfg(not(test))]
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// LVGL backend instance. ZST today — all LVGL state is global (the
/// library itself, plus our static `BAND_BUF`, handle table, listener
/// slots, and event ring). The struct exists to give the trait impl a
/// receiver and to make a future state-bearing backend a one-line change.
#[cfg(not(test))]
pub struct LvglGfx;

#[cfg(not(test))]
impl LvglGfx {
    pub const fn new() -> Self {
        LvglGfx
    }
}

#[cfg(not(test))]
impl Default for LvglGfx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(test))]
impl Gfx for LvglGfx {
    // ── lifecycle ───────────────────────────────────────────────────────────

    fn init(&mut self, width: u16, height: u16) {
        // Cortex-M0+ lacks atomic CAS, so use load + store instead of `swap`;
        // single-threaded JVM contract means this is race-free in practice.
        if INITIALIZED.load(Ordering::Relaxed) {
            return;
        }
        INITIALIZED.store(true, Ordering::Relaxed);
        lifecycle::init(width, height);
        events::init_keypad();
    }

    fn tick(&mut self, ms: u32) {
        lifecycle::tick(ms);
        // Drive toast auto-dismiss + property animations off the same
        // per-frame heartbeat. Done here rather than inside
        // `lifecycle::tick` so the LVGL FFI calls and the picodroid-
        // specific bookkeeping stay in sibling modules (`lvgl::lifecycle`
        // owns LVGL; the others own their own state).
        widgets::toast::tick(ms);
        widgets::snackbar::tick(ms);
        animations::tick(ms);
    }

    fn sleep(&mut self) {
        lifecycle::sleep();
    }

    fn wake(&mut self) {
        lifecycle::wake();
    }

    fn screen(&self) -> Handle {
        lifecycle::screen_handle()
    }

    // ── cross-widget view ops ───────────────────────────────────────────────

    fn set_pos(&mut self, h: Handle, x: i32, y: i32) {
        view_ops::set_pos(h, x, y);
    }

    fn set_size(&mut self, h: Handle, w: i32, height: i32) {
        view_ops::set_size(h, w, height);
    }

    fn set_bg_color(&mut self, h: Handle, argb: u32) {
        view_ops::set_bg_color(h, argb);
    }
    fn set_bg_tint(&mut self, h: Handle, argb: u32) {
        view_ops::set_bg_tint(h, argb);
    }

    fn set_padding(&mut self, h: Handle, l: i32, t: i32, r: i32, b: i32) {
        view_ops::set_padding(h, l, t, r, b);
    }

    fn set_visibility(&mut self, h: Handle, v: Visibility) {
        view_ops::set_visibility(h, v);
    }

    fn set_enabled(&mut self, h: Handle, on: bool) {
        view_ops::set_enabled(h, on);
    }

    fn set_alpha(&mut self, h: Handle, alpha: u8) {
        view_ops::set_alpha(h, alpha);
    }

    fn set_parent(&mut self, h: Handle, parent: Handle) -> Reparent {
        view_ops::set_parent(h, parent)
    }

    fn delete(&mut self, h: Handle) {
        view_ops::delete(h);
    }

    // ── ViewGroup ops ───────────────────────────────────────────────────────

    fn child_count(&self, h: Handle) -> i32 {
        view_ops::child_count(h)
    }

    fn remove_child(&mut self, parent: Handle, child: Handle) {
        view_ops::remove_child(parent, child);
    }

    fn remove_all_children(&mut self, h: Handle) {
        view_ops::remove_all_children(h);
    }

    fn set_flex_grow(&mut self, h: Handle, weight: i32) {
        view_ops::set_flex_grow(h, weight);
    }

    fn set_margins(&mut self, h: Handle, left: i32, top: i32, right: i32, bottom: i32) {
        view_ops::set_margins(h, left, top, right, bottom);
    }

    fn set_min_size(&mut self, h: Handle, width: i32, height: i32) {
        view_ops::set_min_size(h, width, height);
    }

    fn set_max_width(&mut self, h: Handle, width: i32) {
        view_ops::set_max_width(h, width);
    }

    fn set_frame_gravity(&mut self, h: Handle, gravity: i32, dx: i32, dy: i32) {
        view_ops::set_frame_gravity(h, gravity, dx, dy);
    }

    fn frame(&mut self, h: Handle) -> (i32, i32, i32, i32) {
        view_ops::frame(h)
    }

    fn set_view_property(&mut self, h: Handle, p: ViewProperty, value: f32) {
        animations::set_property(h.to_java(), property_code(p), value);
    }

    fn get_view_property(&mut self, h: Handle, p: ViewProperty) -> f32 {
        animations::get_property(h.to_java(), property_code(p))
    }
}

/// Map the backend-neutral property onto the animation engine's code, so the
/// immediate setters and `ViewPropertyAnimator` share one unit conversion and
/// one LVGL write path.
#[cfg(not(test))]
fn property_code(p: ViewProperty) -> i32 {
    match p {
        ViewProperty::Alpha => animations::PROPERTY_ALPHA,
        ViewProperty::TranslationX => animations::PROPERTY_TRANSLATION_X,
        ViewProperty::TranslationY => animations::PROPERTY_TRANSLATION_Y,
        ViewProperty::Rotation => animations::PROPERTY_ROTATION,
        ViewProperty::ScaleX => animations::PROPERTY_SCALE_X,
        ViewProperty::ScaleY => animations::PROPERTY_SCALE_Y,
    }
}

// ── global accessor ─────────────────────────────────────────────────────────
//
// Mirrors today's static-state shape — the LVGL library is global, our
// `BAND_BUF` is global, and the handle table is global. A single static
// `LvglGfx` matches that lifetime and avoids any per-call alloc.

#[cfg(not(test))]
static mut GFX: LvglGfx = LvglGfx::new();

/// Whether `lv_init` has run: false for a plain main-class app that never
/// touched the display, where every LVGL query is undefined.
#[cfg(not(test))]
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}
/// No LVGL in the host test binary (the `window` tests exercise only the
/// placement sums).
#[cfg(test)]
pub fn is_initialized() -> bool {
    false
}

/// Whether the keypad-focused view has an `OnLongClickListener` of its own
/// — then a held SELECT is its long click, not the options menu's opener
/// (`Activity.nativeFocusTakesLongPress`, app-portability K9).
#[cfg(not(test))]
pub fn focused_view_takes_long_press() -> bool {
    // SAFETY: LVGL getters on the UI task; a null group or focus reads as none.
    let focused = unsafe {
        let group = crate::lvgl_ffi::lv_group_get_default();
        if group.is_null() {
            return false;
        }
        crate::lvgl_ffi::lv_group_get_focused(group)
    };
    !focused.is_null() && widgets::button::lookup_long_click_obj(focused as usize).is_some()
}

/// The LVGL pool's `(free, total)` bytes. UI task only, like every LVGL call.
#[cfg(not(test))]
pub fn pool_free_bytes() -> (usize, usize) {
    let mut mon = crate::lvgl_ffi::lv_mem_monitor_t::default();
    // SAFETY: `mon` is a valid out-parameter; LVGL is initialised before any
    // Activity runs, and this is the UI task.
    unsafe { crate::lvgl_ffi::lv_mem_monitor(&mut mon) };
    (mon.free_size, mon.total_size)
}

/// Run a closure with mutable access to the global graphics backend.
///
/// Single-threaded by contract: only the UI task may touch the widget tree
/// (`native_handler::graphics::dispatch` warns any other caller once). Do
/// **not** call this from inside an LVGL `extern "C"` callback —
/// the trampoline would re-borrow and panic. Trampolines must read directly
/// from the per-handle slot tables in `lvgl/events.rs`.
#[cfg(not(test))]
pub fn with_gfx<R>(f: impl FnOnce(&mut dyn Gfx) -> R) -> R {
    // SAFETY: single-threaded access to a `'static mut` singleton; same
    // contract as the existing global state in `engine.rs` (SCREEN_HOLDER,
    // KEY_LISTENERS, etc.) which this is replacing.
    unsafe {
        let gfx: &mut LvglGfx = &mut *core::ptr::addr_of_mut!(GFX);
        f(gfx)
    }
}
