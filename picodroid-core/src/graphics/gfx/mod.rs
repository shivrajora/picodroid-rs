// SPDX-License-Identifier: GPL-3.0-only
//! Backend-agnostic graphics surface.
//!
//! `gfx::Gfx` is the engine-level trait that hides the underlying display
//! library (today: LVGL) from the widgets, view, and display layers. The
//! single impl lives in `super::lvgl`.

pub mod handle;
pub mod trait_def;

pub use handle::Handle;
pub use trait_def::{Gfx, Visibility};
