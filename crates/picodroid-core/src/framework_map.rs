// SPDX-License-Identifier: GPL-3.0-only
//! The shrink-map version this firmware was built against.
//!
//! Active shrink-map version for this build, from the highest committed
//! `sdk/shrink-maps/v<semver>.toml` ≤ the workspace version; `"0.0.0"` (no
//! shrinking) when none exists.
//!
//! It sits in its own module rather than in [`crate::boot`] because it is a
//! build artifact with no dependency on the JVM or the graphics tree, while
//! `boot` is `cfg(not(test))` on account of both. The install path compares a
//! PAPK's version against this one and is compiled in test builds, so the
//! constant has to be too. `boot` re-exports it at its original path.

// Defines: pub const FRAMEWORK_MAP_VERSION.
include!(concat!(env!("OUT_DIR"), "/framework_mapping_version.rs"));
