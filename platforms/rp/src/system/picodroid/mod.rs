// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(test))]
// The whole graphics tree — the backend-neutral gfx seam, the LVGL engine,
// and the Java-facing binding layer — now lives in picodroid-core.
// Re-exported so `crate::system::picodroid::graphics::…` keeps resolving.
pub use picodroid_core::graphics;
// Compiled under test too: hardware/sensors/mailbox.rs is pure atomics and
// carries host unit tests (the JVM-facing natives inside stay
// `cfg(not(test))`).
pub mod hardware;
#[cfg(all(not(test), has_network))]
pub mod net;
#[cfg(not(test))]
pub mod os;
pub mod pio;
#[cfg(not(test))]
pub mod util;
