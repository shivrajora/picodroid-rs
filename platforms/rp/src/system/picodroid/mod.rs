// SPDX-License-Identifier: GPL-3.0-only
//! Re-exports of the framework's Java-facing subsystems, which now live in
//! `picodroid-core`. Keeping the old paths resolving means the JVM's native
//! dispatch arms and every call site are untouched by the extraction.

#[cfg(not(test))]
// The whole graphics tree — the backend-neutral gfx seam, the LVGL engine,
// and the Java-facing binding layer — now lives in picodroid-core.
// Re-exported so `crate::system::picodroid::graphics::…` keeps resolving.
pub use picodroid_core::graphics;
#[cfg(all(not(test), has_network))]
pub use picodroid_core::net;
#[cfg(not(test))]
pub use picodroid_core::os;
pub use picodroid_core::pio;
#[cfg(not(test))]
pub use picodroid_core::util;

// Still here: `sensors` takes `&mut PicodroidNativeHandler` to invoke Java
// listener callbacks, and `native_handler` dispatches into `sensors` — a
// mutual dependency that would become a circular crate dependency if the
// two were split across crates. It moves with native_handler next stage.
//
// Compiled under test too: hardware/sensors/mailbox.rs is pure atomics and
// carries host unit tests (the JVM-facing natives inside stay
// `cfg(not(test))`).
pub mod hardware;
