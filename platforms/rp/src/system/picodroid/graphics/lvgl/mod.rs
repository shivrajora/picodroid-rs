// SPDX-License-Identifier: GPL-3.0-only
//! LVGL backend — the only [`Gfx`] impl today.
//!
//! All `crate::lvgl_ffi::*` imports live under this module after the
//! migration completes; nothing outside `lvgl/` should reference
//! `lv_obj_t` / `lv_event_t` / `lv_color_t`.

use core::sync::atomic::{AtomicBool, Ordering};

use super::gfx::{EventKind, EventListener, EventRecord, Gfx, Handle, Visibility};

pub mod animations;
pub mod calibration;
pub mod drawable;
pub mod events;
pub mod fps_overlay;
pub mod handle_table;
pub mod lifecycle;
pub mod view_ops;
pub mod widgets;

/// Idempotency guard for [`LvglGfx::init`]. LVGL itself doesn't tolerate
/// `lv_init()` twice; this flag latches on the first successful call so
/// repeated `with_gfx(|g| g.init(...))` from `Display.getInstance` and
/// across PDB app reloads are no-ops.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// LVGL backend instance. ZST today — all LVGL state is global (the
/// library itself, plus our static `BAND_BUF`, handle table, listener
/// slots, and event ring). The struct exists to give the trait impl a
/// receiver and to make a future state-bearing backend a one-line change.
pub struct LvglGfx;

impl LvglGfx {
    pub const fn new() -> Self {
        LvglGfx
    }
}

impl Default for LvglGfx {
    fn default() -> Self {
        Self::new()
    }
}

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

    // ── events ──────────────────────────────────────────────────────────────

    fn add_event_listener(&mut self, _h: Handle, _kind: EventKind, _cb: EventListener) {
        unimplemented!("LvglGfx::add_event_listener: ported in step 5 of the plan")
    }

    fn poll_event(&mut self) -> Option<EventRecord> {
        unimplemented!("LvglGfx::poll_event: ported in step 5 of the plan")
    }
}

// ── global accessor ─────────────────────────────────────────────────────────
//
// Mirrors today's static-state shape — the LVGL library is global, our
// `BAND_BUF` is global, and the handle table is global. A single static
// `LvglGfx` matches that lifetime and avoids any per-call alloc.

static mut GFX: LvglGfx = LvglGfx::new();

/// Run a closure with mutable access to the global graphics backend.
///
/// Single-threaded today (the JVM holds the only frontend); a future SMP
/// world will gate this with the same hardware spinlock used by
/// `SharedJvmHeap`. Do **not** call this from inside an LVGL `extern "C"`
/// callback — the trampoline would re-borrow and panic. Trampolines must
/// read directly from the per-handle slot tables in `lvgl/events.rs`.
#[allow(dead_code)] // wired up as widgets migrate (steps 6+)
pub fn with_gfx<R>(f: impl FnOnce(&mut dyn Gfx) -> R) -> R {
    // SAFETY: single-threaded access to a `'static mut` singleton; same
    // contract as the existing global state in `engine.rs` (SCREEN_HOLDER,
    // KEY_LISTENERS, etc.) which this is replacing.
    unsafe {
        let gfx: &mut LvglGfx = &mut *core::ptr::addr_of_mut!(GFX);
        f(gfx)
    }
}
