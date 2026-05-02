// SPDX-License-Identifier: GPL-3.0-only
//! LittleFS block-device backend for the on-chip NOR flash region carved
//! out in the linker script (`FS_FLASH`).
//!
//! Reads are XIP-direct (memcpy from memory-mapped flash).  Writes and
//! erases disable XIP and invoke ROM flash routines — the dangerous bits
//! live in [`crate::hal::flash`] and are already proven by the PAPK
//! install path.

use littlefs_rust::{Error as LfsError, Storage as LfsStorage};

use crate::hal::flash;

pub const BLOCK_SIZE: usize = flash::FLASH_SECTOR_SIZE; // 4096
pub const PROG_SIZE: usize = flash::FLASH_PAGE_SIZE; // 256
pub const READ_SIZE: usize = 16;

pub struct FlashStorage {
    start_offset: u32,
    block_count: u32,
}

impl FlashStorage {
    pub fn new() -> Self {
        let (start_offset, len) = flash::fs_region_bounds();
        Self {
            start_offset,
            block_count: len / BLOCK_SIZE as u32,
        }
    }

    pub fn block_count(&self) -> u32 {
        self.block_count
    }

    fn resolve(&self, block: u32, offset: u32, len: usize) -> Result<u32, LfsError> {
        if block >= self.block_count || (offset as usize) + len > BLOCK_SIZE {
            return Err(LfsError::Invalid);
        }
        Ok(self.start_offset + block * BLOCK_SIZE as u32 + offset)
    }
}

impl Default for FlashStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl LfsStorage for FlashStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        let addr = self.resolve(block, offset, buf.len())?;
        // Safety: addr is within the FS region and XIP is enabled in task
        // context.  Concurrent writes on other cores are prevented by the
        // fs-level mutex (see `fs::with_fs`).
        unsafe { flash::flash_read(addr, buf) };
        Ok(())
    }

    fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
        if !(offset as usize).is_multiple_of(PROG_SIZE) || !data.len().is_multiple_of(PROG_SIZE) {
            return Err(LfsError::Invalid);
        }
        let addr = self.resolve(block, offset, data.len())?;
        unsafe { flash::flash_program_range(addr, data.as_ptr(), data.len()) };
        Ok(())
    }

    fn erase(&mut self, block: u32) -> Result<(), LfsError> {
        let addr = self.resolve(block, 0, BLOCK_SIZE)?;
        unsafe { flash::flash_erase_range(addr, BLOCK_SIZE) };
        Ok(())
    }
}
