// SPDX-License-Identifier: GPL-3.0-only
//! [`crate::hal::HalFs`] over LittleFS.
//!
//! A family that mounts LittleFS registers this with
//! `set_hal_fs!(picodroid_core::fs::LittleFsHal)` instead of writing the
//! ninety-odd lines below. Every method re-resolves its path inside
//! [`with_fs`], whose closure borrows the filesystem for exactly the
//! operation's duration — which is why `HalFs` hands out no file handles.
//!
//! `with_fs` returns `None` when the mount failed, which folds into the same
//! "failed" value the Java API reports (`false`, `0`, `-1`), because
//! `java.io.File`'s predicates cannot throw.

use alloc::vec::Vec;

use littlefs_rust::{FileType, OpenFlags, SeekFrom};

use super::with_fs;
use crate::hal::HalFs;

/// LittleFS, as the framework's file API sees it.
pub struct LittleFsHal;

impl HalFs for LittleFsHal {
    fn exists(path: &str) -> bool {
        with_fs(|fs| fs.exists(path)).unwrap_or(false)
    }

    fn is_file(path: &str) -> bool {
        with_fs(|fs| matches!(fs.stat(path).map(|m| m.file_type), Ok(FileType::File)))
            .unwrap_or(false)
    }

    fn is_dir(path: &str) -> bool {
        with_fs(|fs| matches!(fs.stat(path).map(|m| m.file_type), Ok(FileType::Dir)))
            .unwrap_or(false)
    }

    fn length(path: &str) -> i64 {
        with_fs(|fs| fs.stat(path).map(|m| m.size as i64).unwrap_or(0)).unwrap_or(0)
    }

    fn delete(path: &str) -> bool {
        with_fs(|fs| fs.remove(path).is_ok()).unwrap_or(false)
    }

    fn mkdir(path: &str) -> bool {
        with_fs(|fs| fs.mkdir(path).is_ok()).unwrap_or(false)
    }

    fn rename(from: &str, to: &str) -> bool {
        with_fs(|fs| fs.rename(from, to).is_ok()).unwrap_or(false)
    }

    fn truncate(path: &str) {
        let _ = with_fs(|fs| fs.write_file(path, &[]));
    }

    fn read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32 {
        with_fs(|fs| {
            let file = match fs.open(path, OpenFlags::READ) {
                Ok(f) => f,
                Err(_) => return -1i32,
            };
            if file.seek(SeekFrom::Start(pos as u32)).is_err() {
                return -1;
            }
            let mut tmp = alloc::vec![0u8; len];
            match file.read(&mut tmp) {
                Ok(n) => {
                    out.extend_from_slice(&tmp[..n as usize]);
                    n as i32
                }
                Err(_) => -1,
            }
        })
        .unwrap_or(-1)
    }

    fn write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
        with_fs(|fs| {
            let file = match fs.open(path, OpenFlags::WRITE | OpenFlags::CREATE) {
                Ok(f) => f,
                Err(_) => return -1i32,
            };
            if file.seek(SeekFrom::Start(pos as u32)).is_err() {
                return -1;
            }
            match file.write(data) {
                Ok(n) => {
                    let _ = file.sync();
                    n as i32
                }
                Err(_) => -1,
            }
        })
        .unwrap_or(-1)
    }
}
