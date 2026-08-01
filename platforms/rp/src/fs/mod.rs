// SPDX-License-Identifier: GPL-3.0-only
//! This family's end of the filesystem seam.
//!
//! The filesystem itself — mount recovery, the singleton, the worker, the
//! `HalFs` impl and the simulator's host image — lives in
//! [`picodroid_core::fs`] behind that crate's `littlefs` feature
//! (`docs/designs/family-neutral-residue.md` §3.H). What stays here is what is
//! genuinely ours: the flash geometry and the `__fs_start`/`__fs_end` linker
//! symbols in [`storage`], and the choice of which backing store to mount.

#[cfg(not(feature = "sim"))]
pub mod storage;

pub use picodroid_core::fs::FsError;

/// Mount the filesystem, formatting on first boot or after corruption.
///
/// Must be called exactly once, before `FreeRtosUtils::start_scheduler`.
pub fn init() -> Result<(), FsError> {
    #[cfg(not(feature = "sim"))]
    {
        picodroid_core::fs::init_device(storage::FlashStorage::new())
    }
    // The simulator mounts a host-file image with the same block layout, so
    // its bytes stay interchangeable with a flash dump.
    #[cfg(feature = "sim")]
    {
        picodroid_core::fs::init_host_image()
    }
}
