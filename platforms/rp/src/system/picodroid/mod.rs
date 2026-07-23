// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(test))]
pub mod graphics;
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
