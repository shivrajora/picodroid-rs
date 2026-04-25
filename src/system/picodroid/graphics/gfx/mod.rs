//! Backend-agnostic graphics surface.
//!
//! `gfx::Gfx` is the engine-level trait that hides the underlying display
//! library (today: LVGL) from the widgets, view, and display layers. The
//! single impl lives in `super::lvgl`. See the
//! `decouple-graphics-from-lvgl` plan for the migration shape.

// Skeleton: items become live as widgets and view-ops migrate (plan steps 2+).
#![allow(dead_code, unused_imports)]

pub mod handle;
pub mod trait_def;
pub mod widget_ops;

pub use handle::Handle;
pub use trait_def::{EventKind, EventListener, EventPayload, EventRecord, Gfx, Visibility};
