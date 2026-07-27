// SPDX-License-Identifier: GPL-3.0-only
//! Simulator flash stub — there is no XIP flash region to erase or program.
//!
//! Only the two constants exist, and only so the module has the same shape as
//! a family's real one; nothing calls them, because `packagemanager` is gated
//! out of simulator builds.
//!
//! Both values are picodroid-wide rather than family-specific: the magic is
//! the PAPK boot-meta identifier, and the size ceiling is a board/linker
//! number. A family whose simulator models a real flash region defines its
//! own `flash` module.

/// PAPK boot-meta magic — `"PDB1"` as a little-endian `u32`.
#[allow(dead_code)]
pub const PAPK_FLASH_MAGIC: u32 = 0x5044_4231;

/// Largest PAPK the install path will accept.
#[allow(dead_code)]
pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024;
