// SPDX-License-Identifier: GPL-3.0-only
//! Host-only helpers for the text-based guard tests in `picodroid-core` and
//! each platform crate. A dev-dependency of both, so the walker and the
//! comment stripper exist once and the guards cannot drift apart.

pub mod gc_root_scan;
pub mod source_scan;
