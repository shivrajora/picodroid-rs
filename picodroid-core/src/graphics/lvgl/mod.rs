// SPDX-License-Identifier: GPL-3.0-only
//! LVGL backend — the only [`Gfx`] impl today.
//!
//! Nothing outside `lvgl/` should reference `lv_obj_t` / `lv_event_t` /
//! `lv_color_t`; the rest of the graphics layer goes through [`Gfx`] and
//! opaque [`Handle`]s.

#[cfg_attr(test, allow(unused_imports))]
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg_attr(test, allow(unused_imports))]
use super::gfx::{Gfx, Handle, Visibility};

// `lvgl_ffi`'s `extern "C"` block is `cfg(not(test))`, so its drift-check
// tests can run without linking LVGL. Every module that calls an LVGL
// function follows the same gate; the ones below it are host-testable
// because they touch only `LV_KEY_*` constants or stub the FFI under test
// (see `handle_table`'s opaque `lv_obj_t`).
#[cfg(not(test))]
pub mod animations;
#[cfg(not(test))]
pub mod calibration;
#[cfg(not(test))]
pub mod drawable;
#[cfg(not(test))]
pub mod events;
#[cfg(not(test))]
pub mod fps_overlay;
#[cfg(not(test))]
pub mod lifecycle;
#[cfg(not(test))]
pub mod view_ops;
#[cfg(not(test))]
pub mod widgets;

pub mod edit_mode;
pub mod handle_table;
pub mod key_debounce;
pub mod key_filter;
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

    fn set_parent(&mut self, h: Handle, parent: Handle) {
        view_ops::set_parent(h, parent);
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

    fn frame(&mut self, h: Handle) -> (i32, i32, i32, i32) {
        view_ops::frame(h)
    }
}

// ── global accessor ─────────────────────────────────────────────────────────
//
// Mirrors today's static-state shape — the LVGL library is global, our
// `BAND_BUF` is global, and the handle table is global. A single static
// `LvglGfx` matches that lifetime and avoids any per-call alloc.

#[cfg(not(test))]
static mut GFX: LvglGfx = LvglGfx::new();

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
