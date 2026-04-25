//! LVGL backend — the only [`Gfx`] impl today.
//!
//! All `crate::lvgl_ffi::*` imports live under this module after the
//! migration completes; nothing outside `lvgl/` should reference
//! `lv_obj_t` / `lv_event_t` / `lv_color_t`.

use super::gfx::{EventKind, EventListener, EventRecord, Gfx, Handle, Visibility};

pub mod handle_table;
pub mod lifecycle;

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
        lifecycle::init(width, height);
    }

    fn tick(&mut self, ms: u32) {
        lifecycle::tick(ms);
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

    fn set_pos(&mut self, _h: Handle, _x: i32, _y: i32) {
        unimplemented!("LvglGfx::set_pos: ported in step 6 of the plan")
    }

    fn set_size(&mut self, _h: Handle, _w: i32, _height: i32) {
        unimplemented!("LvglGfx::set_size: ported in step 6 of the plan")
    }

    fn set_bg_color(&mut self, _h: Handle, _argb: u32) {
        unimplemented!("LvglGfx::set_bg_color: ported in step 6 of the plan")
    }

    fn set_padding(&mut self, _h: Handle, _l: i32, _t: i32, _r: i32, _b: i32) {
        unimplemented!("LvglGfx::set_padding: ported in step 6 of the plan")
    }

    fn set_visibility(&mut self, _h: Handle, _v: Visibility) {
        unimplemented!("LvglGfx::set_visibility: ported in step 6 of the plan")
    }

    fn set_enabled(&mut self, _h: Handle, _on: bool) {
        unimplemented!("LvglGfx::set_enabled: ported in step 6 of the plan")
    }

    fn set_alpha(&mut self, _h: Handle, _alpha: u8) {
        unimplemented!("LvglGfx::set_alpha: ported in step 6 of the plan")
    }

    fn set_parent(&mut self, _h: Handle, _parent: Handle) {
        unimplemented!("LvglGfx::set_parent: ported in step 6 of the plan")
    }

    fn delete(&mut self, _h: Handle) {
        unimplemented!("LvglGfx::delete: ported in step 6 of the plan")
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
