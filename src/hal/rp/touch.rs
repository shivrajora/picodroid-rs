//! Touch facade — delegates to the generic XPT2046 driver via board config.
//!
//! Preserves the free-function API (`hal::touch::init()`, etc.) so that
//! `engine.rs` and LVGL callbacks need zero changes.

use crate::boards;
use core::ptr::addr_of_mut;

static mut TOUCH: Option<boards::Touch> = None;

/// Initialize the touch controller via the board-specific driver.
pub fn init() {
    unsafe {
        addr_of_mut!(TOUCH).write(Some(boards::create_touch()));
    }
}

/// Read calibrated screen coordinates (0..WIDTH-1, 0..HEIGHT-1).
/// Returns `None` if no touch is active.
pub fn read_point() -> Option<(u16, u16)> {
    unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap().read_point() }
}
