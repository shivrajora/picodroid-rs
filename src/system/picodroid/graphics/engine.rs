//! Compatibility shim for the legacy `engine::*` API.
//!
//! The LVGL lifecycle lives in [`super::lvgl::lifecycle`] and the keypad
//! event queue lives in [`super::lvgl::events`], both behind the
//! [`super::gfx::Gfx`] trait. This module preserves the call shape used by
//! `app::run_jvm_with`, `display.rs`, `widgets/*.rs`, and `calibration.rs`
//! during the multi-step migration. Once widgets switch to
//! `with_gfx(|g| g.screen())` (plan step 7) and `display.rs` follows
//! (step 8), this file's screen accessor goes away (step 10).

use crate::hal;
use crate::lvgl_ffi::lv_obj_t;
use core::sync::atomic::{AtomicBool, Ordering};

use super::lvgl::{lifecycle, with_gfx};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize LVGL, the display, the touch controller, and the keypad
/// indev. Idempotent.
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    with_gfx(|g| g.init(hal::display::WIDTH, hal::display::HEIGHT));
}

/// Advance LVGL's internal tick counter and process pending timers /
/// rendering. Call periodically (~16 ms for 60 fps).
pub fn tick(ms: u32) {
    with_gfx(|g| g.tick(ms));
}

/// Put the display panel into low-power sleep. LVGL state is untouched —
/// the caller is responsible for stopping `tick()` until `wake()`.
#[cfg_attr(any(feature = "sim", not(has_buttons)), allow(dead_code))]
pub fn sleep() {
    with_gfx(|g| g.sleep());
}

/// Bring the display back from sleep and force a full LVGL repaint.
#[cfg_attr(any(feature = "sim", not(has_buttons)), allow(dead_code))]
pub fn wake() {
    with_gfx(|g| g.wake());
}

/// Get the active screen object as a raw LVGL pointer.
///
/// Legacy accessor used by widgets that haven't been migrated to
/// `with_gfx(|g| g.screen())` yet (plan step 7). Goes away in step 10.
pub fn screen() -> *mut lv_obj_t {
    lifecycle::screen_ptr()
}

/// Re-export calibration entry point so existing `engine::calibrate()`
/// calls continue to work without changes.
pub use super::calibration::calibrate;

// Re-export the keypad/event-queue API from its new home in `lvgl::events`
// so external callers in `app.rs` and `lifecycle.rs` keep compiling.
pub use super::lvgl::events::{
    drain_key_event, focused_view_obj, pin_to_keycode, reset_key_event_queue,
};
