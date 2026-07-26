// SPDX-License-Identifier: GPL-3.0-only
//! Graphics: the backend-neutral surface and its LVGL implementation.
//!
//! [`gfx`] is the seam the rest of the framework draws through — opaque
//! handles, an event model, and the [`Gfx`](gfx::Gfx) trait. [`lvgl`] is the
//! only implementation today, and owns every `lv_*` call in the codebase.
//! [`view`], [`display`], [`widgets`] and friends are the Java-facing
//! binding layer that the JVM's native dispatch calls into.
//!
//! The binding layer moved in the same commit as the engine rather than
//! after it: the two are mutually dependent (widgets reach into the LVGL
//! engine, the engine's view-ops reach back into keyboard/number-picker
//! widgets), and a split would have needed a core → platform call, which is
//! a circular crate dependency. Keeping them together also preserves the
//! `pub(in crate::graphics)` visibility discipline, which cannot span
//! crates and would otherwise have had to widen to `pub`.

pub mod gfx;
pub mod lvgl;

// The binding layer reaches into the LVGL engine, so it carries the same
// `cfg(not(test))` gate that `lvgl_ffi`'s `extern "C"` block does — see
// [`lvgl`]. `gfx` stays ungated: it is the backend-neutral seam and names no
// LVGL type.
#[cfg(not(test))]
pub mod assets;
#[cfg(not(test))]
pub mod display;
#[cfg(not(test))]
pub mod fields;
#[cfg(not(test))]
pub mod view;
#[cfg(not(test))]
pub mod view_group;
#[cfg(not(test))]
pub mod widgets;
