// SPDX-License-Identifier: GPL-3.0-only
//! Simulator flash stub — there is no XIP flash region to erase or program.
//!
//! Only the two constants exist, and only so the module has the same shape as
//! a family's real one; nothing calls them, because `packagemanager` is gated
//! out of simulator builds.
//!
//! Neither value is family-specific: the magic belongs to the PAPK boot-meta
//! format, and the size ceiling is a board/linker number. A family whose
//! simulator models a real flash region defines its own `flash` module.

/// PAPK boot-meta magic — re-exported rather than restated, since a
/// simulator that disagreed with the format crate about it would be
/// modelling a device that cannot exist.
#[allow(dead_code)]
pub use papk_format::flash_image::MAGIC as PAPK_FLASH_MAGIC;

/// Largest PAPK the install path will accept.
#[allow(dead_code)]
pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024;
