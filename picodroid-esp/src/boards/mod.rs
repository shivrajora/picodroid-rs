// SPDX-License-Identifier: GPL-3.0-only
// Board glue for picodroid-esp. Currently only T-Deck Plus is supported.
pub mod tdeck_plus;
#[allow(unused_imports)]
pub use tdeck_plus as board;
