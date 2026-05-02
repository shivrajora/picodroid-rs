// SPDX-License-Identifier: GPL-3.0-only
//! Host-file-backed LittleFS storage for the simulator.
//!
//! Mirrors the on-device block layout ([`BLOCK_SIZE`], [`PROG_SIZE`],
//! [`READ_SIZE`]) so the image format is byte-for-byte compatible with a
//! flash dump — the same bytes could, in principle, be written to the
//! device's `FS_FLASH` region.

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use littlefs_rust::{Error as LfsError, Storage as LfsStorage};

pub const BLOCK_SIZE: usize = 4096;
pub const PROG_SIZE: usize = 256;
pub const READ_SIZE: usize = 16;

const DEFAULT_SIZE_KB: u32 = 256;

pub struct HostFileStorage {
    file: File,
    block_count: u32,
}

impl HostFileStorage {
    pub fn new() -> std::io::Result<Self> {
        let path = resolve_path();
        let size_kb: u32 = env::var("PICODROID_SIM_FS_KB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_SIZE_KB);
        let total_len: u64 = u64::from(size_kb) * 1024;
        assert!(
            total_len.is_multiple_of(BLOCK_SIZE as u64) && total_len > 0,
            "PICODROID_SIM_FS_KB must be a positive multiple of 4"
        );
        let block_count = (total_len / BLOCK_SIZE as u64) as u32;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let current_len = file.metadata()?.len();
        if current_len < total_len {
            // Extend with 0xFF so a fresh image looks erased (LittleFS
            // format expects erased flash to read as 0xFF).
            file.seek(SeekFrom::Start(current_len))?;
            let fill = [0xFFu8; 4096];
            let mut remaining = total_len - current_len;
            while remaining > 0 {
                let n = remaining.min(fill.len() as u64) as usize;
                file.write_all(&fill[..n])?;
                remaining -= n as u64;
            }
            file.sync_all()?;
        }

        Ok(Self { file, block_count })
    }

    pub fn block_count(&self) -> u32 {
        self.block_count
    }

    fn seek_block(&mut self, block: u32, offset: u32, len: usize) -> Result<(), LfsError> {
        if block >= self.block_count || (offset as usize) + len > BLOCK_SIZE {
            return Err(LfsError::Invalid);
        }
        let pos = u64::from(block) * BLOCK_SIZE as u64 + u64::from(offset);
        self.file
            .seek(SeekFrom::Start(pos))
            .map_err(|_| LfsError::Io)?;
        Ok(())
    }
}

fn resolve_path() -> PathBuf {
    if let Ok(p) = env::var("PICODROID_SIM_FS") {
        return PathBuf::from(p);
    }
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push("sim-fs.img");
    p
}

impl LfsStorage for HostFileStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        self.seek_block(block, offset, buf.len())?;
        self.file.read_exact(buf).map_err(|_| LfsError::Io)
    }

    fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
        if !(offset as usize).is_multiple_of(PROG_SIZE) || !data.len().is_multiple_of(PROG_SIZE) {
            return Err(LfsError::Invalid);
        }
        self.seek_block(block, offset, data.len())?;
        self.file.write_all(data).map_err(|_| LfsError::Io)?;
        self.file.sync_all().map_err(|_| LfsError::Io)
    }

    fn erase(&mut self, block: u32) -> Result<(), LfsError> {
        self.seek_block(block, 0, BLOCK_SIZE)?;
        let erased = [0xFFu8; BLOCK_SIZE];
        self.file.write_all(&erased).map_err(|_| LfsError::Io)?;
        self.file.sync_all().map_err(|_| LfsError::Io)
    }
}
