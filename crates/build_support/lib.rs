// SPDX-License-Identifier: GPL-3.0-only
//! Build-script library shared by `picodroid-core/build.rs` and each
//! platform's `build.rs` — one implementation, one compile, and a test
//! harness (a build script is a plain binary, so a `#[cfg(test)]` module
//! `#[path]`-included into one never runs).
//!
//! `crates/jvm/build.rs` still `#[path]`-includes `jvm_defaults.rs` and
//! `names.rs`: pico-jvm takes no build-dependency on a path crate, so both
//! files must stay free of `crate::` references.

pub mod board_cfg;
pub mod boards;
pub mod config;
pub mod flash_layout;
#[cfg(feature = "freertos-device")]
pub mod freertos;
pub mod freertos_host;
pub mod jvm_defaults;
pub mod lvgl;
pub mod names;
pub mod network;
pub mod papk;
