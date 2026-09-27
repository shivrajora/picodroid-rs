// SPDX-License-Identifier: GPL-3.0-only
//! TLS 1.3 client pieces for picodroid, on `embedded-tls` (vendored under
//! `third_party/embedded-tls`): the compiled-in trust store
//! ([`anchors`]), the multi-anchor verifier ([`verifier`]) and a
//! heap-resident session over any blocking `embedded_io` socket
//! ([`session`]). No HAL, JVM or transport dependencies — picodroid-core's
//! `net::tls` supplies the socket, the wall clock and the entropy, and this
//! crate stays testable on the host (`net` is `cfg(not(test))` there).
//!
//! Design and measurements: `docs/designs/tls-2026-09.md`.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod anchors;
pub mod der_lite;
pub mod rng;
pub mod session;
pub mod verifier;

pub use embedded_io;
pub use embedded_tls::{NoClock, TlsClock, TlsError};
pub use session::{OpenError, TlsSession, READ_RECORD_BYTES, WRITE_RECORD_BYTES};
pub use verifier::Failure;
