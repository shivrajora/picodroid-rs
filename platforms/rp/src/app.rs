// SPDX-License-Identifier: GPL-3.0-only
//! What is left of app startup after the shared parts moved to
//! `picodroid_core::boot`: this family's spelling of `run_app`.
//!
//! Where an app's bytes come from is the package directory's business
//! (`picodroid_core::packages`): the runs in `PAPK_FLASH` and the system
//! apps in `.rodata` on a device, the in-memory region in the simulator.
//! The supervisor loop in `boot_tasks.rs` asks it which image runs next.

// `hal/rp/boot.rs` runs an installed PAPK through this rather than the
// built-in one, so the old name is kept as the local spelling.
#[cfg(not(any(test, feature = "sim")))]
pub use picodroid_core::boot::run_app as run_jvm_with;
