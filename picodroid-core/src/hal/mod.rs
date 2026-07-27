// SPDX-License-Identifier: GPL-3.0-only
//! Hardware abstraction — the seam between shared framework code and a
//! platform crate.
//!
//! Three pieces:
//!
//! * [`traits`] — HAL CONTRACT v2. What a platform must provide.
//! * [`facade`] — what shared code calls (`hal::display::update_window()`),
//!   forwarding to `extern "Rust"` symbols.
//! * [`macros`] — `set_hal_*!`, which a platform invokes to emit the
//!   `#[no_mangle]` shims binding those symbols to its trait impls.
//!
//! Shared code should only ever touch the facade. The traits exist for
//! platform crates; the macros are re-exported at the crate root by
//! `#[macro_export]`.
//!
//! Design and rationale: `docs/designs/shared-core-extraction.md` §3.A.

pub mod types;

mod facade;
mod macros;
mod traits;

pub use facade::*;
pub use traits::{
    HalAdc, HalClock, HalDisplay, HalFs, HalGpio, HalI2c, HalNet, HalPwm, HalSpi, HalTouch, HalUart,
};
