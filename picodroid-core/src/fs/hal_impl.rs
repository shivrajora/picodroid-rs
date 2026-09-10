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
//!
//! `space()` needs LittleFS to walk every file on the volume for the used
//! block count, ~20 ms on an RP2350 for a few dozen files. Every mutating
//! method counts itself, so the walk repeats only once something changed:
//! a screen that asks for the total, the free and the available space pays
//! for one walk, and a screen that asks again pays nothing.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use littlefs_rust::{FileType, OpenFlags, SeekFrom};

use super::with_fs;
use crate::hal::{DirEntry, HalFs};

/// Mutations of the volume since boot, counted on the fs worker.
static MUTATIONS: AtomicU32 = AtomicU32::new(0);
/// The used-block count `space()` last walked, and the mutation count it
/// was walked at (`u32::MAX`: never).
static USED_BLOCKS: AtomicU32 = AtomicU32::new(0);
static USED_AT: AtomicU32 = AtomicU32::new(u32::MAX);

/// Note a change to the volume. Runs inside a `with_fs` closure, which the
/// worker serialises, so a load and a store suffice — the Cortex-M0+ has no
/// read-modify-write atomics.
fn mutated() {
    let next = MUTATIONS.load(Ordering::Relaxed).wrapping_add(1);
    MUTATIONS.store(next, Ordering::Release);
}

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
        with_fs(|fs| {
            mutated();
            fs.remove(path).is_ok()
        })
        .unwrap_or(false)
    }

    fn mkdir(path: &str) -> bool {
        with_fs(|fs| {
            mutated();
            fs.mkdir(path).is_ok()
        })
        .unwrap_or(false)
    }

    fn rename(from: &str, to: &str) -> bool {
        with_fs(|fs| {
            mutated();
            fs.rename(from, to).is_ok()
        })
        .unwrap_or(false)
    }

    fn truncate(path: &str) {
        let _ = with_fs(|fs| {
            mutated();
            fs.write_file(path, &[])
        });
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
            mutated();
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

    fn list_dir(path: &str, out: &mut Vec<DirEntry>) -> bool {
        with_fs(|fs| {
            let Ok(dir) = fs.read_dir(path) else {
                return false;
            };
            for entry in dir {
                let Ok(entry) = entry else {
                    return false;
                };
                // LittleFS lists `.` and `..` first; Java's `list()` does not.
                if entry.name == "." || entry.name == ".." {
                    continue;
                }
                out.push(DirEntry {
                    dir: entry.file_type == FileType::Dir,
                    size: entry.size,
                    name: entry.name,
                });
            }
            true
        })
        .unwrap_or(false)
    }

    fn space() -> (u64, u64) {
        let total = super::volume_bytes();
        let used = with_fs(|fs| {
            let at = MUTATIONS.load(Ordering::Acquire);
            if USED_AT.load(Ordering::Acquire) == at {
                return USED_BLOCKS.load(Ordering::Relaxed);
            }
            let blocks = fs.fs_size().unwrap_or(0);
            USED_BLOCKS.store(blocks, Ordering::Relaxed);
            USED_AT.store(at, Ordering::Release);
            blocks
        })
        .map_or(0, |blocks| {
            u64::from(blocks) * u64::from(super::block_size())
        });
        (total, total.saturating_sub(used))
    }
}
