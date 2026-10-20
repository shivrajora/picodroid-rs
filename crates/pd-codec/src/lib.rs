// SPDX-License-Identifier: GPL-3.0-only
//! The pure decoders `picodroid-core` trusts, without the platform behind
//! them: what a network packet, a `/system/*` store record, an app's path
//! or a compiled layout node means. No HAL, no JVM, no statics.
//!
//! Core re-exports each module under its old path (`time_service::sntp`,
//! `hal::wifi::decode`, `identity::Serial`, `storage::sandbox::resolve`,
//! `graphics::inflate_scan`), so the only reason this is a crate of its own
//! is that the host can test and fuzz the decoders without building LVGL:
//! `crates/fuzz-targets` depends on leaf crates only
//! (docs/designs/fuzzing-2026-10.md §8).
//!
//! Every decoder here refuses rather than guesses. A record that is short,
//! out of range or CRC-dirty is `None`, and the caller keeps its default.

#![cfg_attr(not(test), no_std)]
// `sandbox::resolve` has the crate's one unsafe block.
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod inflate_scan;
pub mod sandbox;
pub mod serial;
pub mod sntp;
pub mod time;
pub mod wifi;
