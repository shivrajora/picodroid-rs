// SPDX-License-Identifier: GPL-3.0-only
//! This family's end of the filesystem seam.
//!
//! The filesystem itself — mount recovery, the singleton, the worker, the
//! `HalFs` impl and the simulator's host image — lives in
//! [`picodroid_core::fs`] behind that crate's `littlefs` feature
//! (`docs/designs/family-neutral-residue.md` §3.H). What stays here is what is
//! genuinely ours: the flash geometry and the `__fs_start`/`__fs_end` linker
//! symbols in [`storage`], and the choice of which backing store to mount.
//! The simulator mounts its host-file image from `picodroid_core::sim_boot`,
//! so this module is device-only.

pub mod storage;

pub use picodroid_core::fs::FsError;

/// Mount the filesystem, formatting on first boot or after corruption.
///
/// Must be called exactly once, before `FreeRtosUtils::start_scheduler`.
pub fn init() -> Result<(), FsError> {
    picodroid_core::fs::init_device(storage::FlashStorage::new())
}
