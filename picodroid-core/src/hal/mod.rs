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

// Java `byte[]` staging for the bus natives — what the `HalI2c` / `HalSpi`
// default methods are built on.
pub mod array_io;
// The GPIO edge ring between an interrupt and the UI task, and the scripted
// touch a debug bridge engages — both were copied per family before.
pub mod event_ring;
pub mod touch_override;

// The host implementation of the hardware surface, shared by every family
// rather than copied into each one — see the module docs for why.
#[cfg(any(test, feature = "sim"))]
pub mod sim;

// The FreeRTOS+TCP socket layer a device family registers instead of writing
// its own (`set_hal_net!(FreeRtosTcpNet)`). Feature-gated like `fs`
// (`littlefs`): a family on another IP stack links none of it. Never in the
// simulator (host sockets) or in core's own tests (no kernel to link).
#[cfg(all(feature = "freertos-tcp", has_network, not(any(test, feature = "sim"))))]
pub mod freertos_tcp;

mod facade;
mod macros;
mod traits;

pub use facade::*;
pub use traits::{
    HalAdc, HalClock, HalDisplay, HalFs, HalGpio, HalI2c, HalNet, HalPwm, HalSpi, HalTouch,
    HalUart, NetLink,
};
