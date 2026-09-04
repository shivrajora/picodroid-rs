// SPDX-License-Identifier: GPL-3.0-only
//! LittleFS block-device backend for the on-chip NOR flash region carved
//! out in the linker script (`FS_FLASH`).
//!
//! Reads are XIP-direct (memcpy from memory-mapped flash).  Writes and
//! erases disable XIP and invoke ROM flash routines — the dangerous bits
//! live in [`crate::hal::flash`] and are already proven by the PAPK
//! install path. The block arithmetic and the alignment rule are
//! [`FsGeometry`]'s, shared with the host image.

use littlefs_rust::{Error as LfsError, Storage as LfsStorage};
use picodroid_core::fs::{FsBackingStore, FsGeometry};

use crate::hal::flash;

/// This flash *is* the default geometry — checked, not assumed.
const GEOMETRY: FsGeometry = FsGeometry::DEFAULT;
const _: () = {
    assert!(GEOMETRY.block as usize == flash::FLASH_SECTOR_SIZE);
    assert!(GEOMETRY.prog as usize == flash::FLASH_PAGE_SIZE);
};

pub struct FlashStorage {
    start_offset: u32,
    block_count: u32,
}

impl FlashStorage {
    pub fn new() -> Self {
        let (start_offset, len) = flash::fs_region_bounds();
        Self {
            start_offset,
            block_count: len / GEOMETRY.block,
        }
    }

    /// Flash-relative offset of `(block, offset)`, checked against the region.
    fn resolve(&self, block: u32, offset: u32, len: usize) -> Result<u32, LfsError> {
        let within = GEOMETRY.resolve(self.block_count, block, offset, len)?;
        Ok(self.start_offset + within as u32)
    }
}

impl Default for FlashStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl FsBackingStore for FlashStorage {
    fn block_count(&self) -> u32 {
        self.block_count
    }
}

impl LfsStorage for FlashStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        let addr = self.resolve(block, offset, buf.len())?;
        // Safety: addr is within the FS region and XIP is enabled in task
        // context.  Concurrent writes on other cores are prevented by the fs
        // worker: every caller reaches this through `picodroid_core::fs::
        // with_fs`, which runs the operation on one core-0-pinned task.
        unsafe { flash::flash_read(addr, buf) };
        Ok(())
    }

    fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
        GEOMETRY.check_prog(offset, data.len())?;
        let addr = self.resolve(block, offset, data.len())?;
        unsafe { flash::flash_program_range(addr, data.as_ptr(), data.len()) };
        Ok(())
    }

    fn erase(&mut self, block: u32) -> Result<(), LfsError> {
        let addr = self.resolve(block, 0, GEOMETRY.block as usize)?;
        unsafe { flash::flash_erase_range(addr, GEOMETRY.block as usize) };
        Ok(())
    }
}
