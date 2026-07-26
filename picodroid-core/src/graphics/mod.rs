// SPDX-License-Identifier: GPL-3.0-only
//! Graphics: the LVGL-backed rendering and widget layer.
//!
//! Being filled in stages. Files land here as their dependencies do, so at
//! any given commit part of the tree still lives in the platform crate and
//! is re-exported from here — see `docs/designs/shared-core-extraction.md`.

pub mod lvgl;
