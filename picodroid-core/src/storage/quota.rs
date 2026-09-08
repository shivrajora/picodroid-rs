// SPDX-License-Identifier: GPL-3.0-only
//! The storage quota (docs/designs/multi-app-2026-09.md D10, §8 P4): a
//! system reserve the apps may not eat into, and a cap on what one app may
//! hold. Multi-app boards only — a single-app board has no system packages
//! to protect and no second app to fence off, so there [`charge`] refuses
//! nothing and [`available_for_app`] is the volume's free space.
//!
//! Accounting is in LittleFS's currency: a file costs `ceil(size / 4 KB)`
//! blocks, a directory its metadata pair (8 KB), the package directory
//! included. The running package's usage is walked once per run — on its
//! first accounted operation, not at `run_app`, so an app that never
//! touches storage pays nothing — and kept as deltas after: the natives
//! charge a growth before they write and credit what a delete or a
//! truncate frees. A system package is exempt from both rules but counted
//! all the same, so `StorageStats` can report it.
//!
//! The counter is shared by every Java thread that reaches the file
//! natives; its read-modify-writes sit in the JVM's scheduler-atomic
//! section, the guard the heap compounds use.

/// LittleFS's block: what a byte of file data is rounded up to.
pub const BLOCK: u64 = 4096;
/// A directory's metadata pair.
pub const DIR_BYTES: u64 = 2 * BLOCK;

/// Why a growth was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The app would exceed `app_data_cap_kb`.
    Cap,
    /// The volume would drop below `fs_system_reserve_kb`.
    Reserve,
}

/// Bytes a file of `len` bytes occupies.
pub fn file_bytes(len: u64) -> u64 {
    len.div_ceil(BLOCK) * BLOCK
}

#[cfg(has_multi_app)]
pub use enforced::{available_for_app, charge, invalidate, usage_of, walk_package};

/// A single-app board keeps no quota: nothing to forget.
#[cfg(not(has_multi_app))]
pub fn invalidate() {}

/// A single-app board keeps no quota: nothing is refused.
#[cfg(not(has_multi_app))]
pub fn charge(_delta: i64) -> Result<(), Refused> {
    Ok(())
}

/// A single-app board keeps no quota: an app may write what is free.
#[cfg(not(has_multi_app))]
pub fn available_for_app() -> u64 {
    crate::hal::fs::space().1
}

#[cfg(has_multi_app)]
mod enforced {
    use super::{file_bytes, Refused, DIR_BYTES};
    use crate::board_cfg::flash::{APP_DATA_CAP_BYTES, FS_SYSTEM_RESERVE_BYTES};
    use crate::hal::{fs, DirEntry};
    use crate::packages;
    use crate::storage::sandbox::{self, BUF};
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicU32, Ordering};
    use pico_jvm::atomic_section::AtomicSection;

    /// How deep the usage walk follows subdirectories.
    const WALK_DEPTH: usize = 4;

    // The running package's usage: which run it was walked for, and the
    // bytes. `u32` holds it: the volume is smaller than 4 GB.
    static USAGE_GENERATION: AtomicU32 = AtomicU32::new(u32::MAX);
    static USAGE_BYTES: AtomicU32 = AtomicU32::new(0);

    /// Forget the running package's walk, so the next accounted operation
    /// walks again: what the sandbox calls once it has made the package
    /// directory, whose pair a walk that ran before it existed never saw.
    pub fn invalidate() {
        USAGE_GENERATION.store(u32::MAX, Ordering::Release);
    }

    /// The running package's usage, walked first when this run has not yet.
    fn usage_of_running() -> u64 {
        let generation = packages::run_generation();
        if USAGE_GENERATION.load(Ordering::Acquire) == generation {
            return u64::from(USAGE_BYTES.load(Ordering::Relaxed));
        }
        let bytes = packages::running().map_or(0, walk_package);
        let _atomic = AtomicSection::enter();
        USAGE_BYTES.store(bytes as u32, Ordering::Relaxed);
        USAGE_GENERATION.store(generation, Ordering::Release);
        bytes
    }

    /// What `/data/<package>` occupies: its directory, every file rounded
    /// to blocks, every subdirectory's pair. 0 when it has no directory.
    pub fn walk_package(package: &str) -> u64 {
        let mut buf = [0u8; BUF];
        let Ok(root) = sandbox::resolve(Some(package), "", &mut buf) else {
            return 0;
        };
        let mut entries = Vec::new();
        if !fs::list_dir(root, &mut entries) {
            return 0;
        }
        DIR_BYTES + walk_entries(root, entries, WALK_DEPTH)
    }

    fn walk_entries(dir: &str, entries: Vec<DirEntry>, depth: usize) -> u64 {
        let mut total = 0;
        for entry in entries {
            if entry.dir {
                total += DIR_BYTES;
                if depth > 0 {
                    let child = alloc::format!("{dir}/{}", entry.name);
                    let mut below = Vec::new();
                    if fs::list_dir(&child, &mut below) {
                        total += walk_entries(&child, below, depth - 1);
                    }
                }
            } else {
                total += file_bytes(u64::from(entry.size));
            }
        }
        total
    }

    /// What `package` holds: the running package's counter, another's walk.
    pub fn usage_of(package: &str) -> u64 {
        if packages::running() == Some(package) {
            usage_of_running()
        } else {
            walk_package(package)
        }
    }

    /// Account `delta` bytes (already block-rounded) to the running package.
    /// A growth is checked against the cap and the reserve first and, when
    /// refused, leaves the counter untouched; a shrink always applies.
    pub fn charge(delta: i64) -> Result<(), Refused> {
        let Some(package) = packages::running() else {
            return Ok(());
        };
        let usage = usage_of_running();
        if delta > 0 && !packages::is_system(package) {
            let growth = delta as u64;
            let cap = APP_DATA_CAP_BYTES as u64;
            if cap > 0 && usage + growth > cap {
                return Err(Refused::Cap);
            }
            let (_, free) = fs::space();
            if free < FS_SYSTEM_RESERVE_BYTES as u64 + growth {
                return Err(Refused::Reserve);
            }
        }
        let _atomic = AtomicSection::enter();
        let now = i64::from(USAGE_BYTES.load(Ordering::Relaxed));
        USAGE_BYTES.store((now + delta).max(0) as u32, Ordering::Relaxed);
        Ok(())
    }

    /// Bytes the running app may still write: the free space above the
    /// reserve, and under its cap. A system package sees the free space.
    pub fn available_for_app() -> u64 {
        let (_, free) = fs::space();
        let Some(package) = packages::running() else {
            return free.saturating_sub(FS_SYSTEM_RESERVE_BYTES as u64);
        };
        if packages::is_system(package) {
            return free;
        }
        let head = free.saturating_sub(FS_SYSTEM_RESERVE_BYTES as u64);
        let cap = APP_DATA_CAP_BYTES as u64;
        if cap > 0 {
            head.min(cap.saturating_sub(usage_of_running()))
        } else {
            head
        }
    }
}

#[cfg(all(test, has_multi_app))]
mod tests {
    use super::*;
    use crate::board_cfg::flash::{APP_DATA_CAP_BYTES, FS_SYSTEM_RESERVE_BYTES};
    use crate::hal::fs;
    use crate::packages;
    use alloc::vec;

    fn blob(path: &str, len: usize) {
        fs::truncate(path);
        fs::write_at(path, 0, &vec![7u8; len]);
    }

    #[test]
    fn usage_counts_blocks_and_directory_pairs() {
        let _g = packages::test_support::lock();
        packages::set_running(Some("com.walk"));
        blob("/data/com.walk/a", 1);
        blob("/data/com.walk/sub/b", 4097);
        // The package directory, the file (one block), the subdirectory's
        // pair, and its file (two blocks).
        assert_eq!(
            walk_package("com.walk"),
            DIR_BYTES + BLOCK + DIR_BYTES + 2 * BLOCK
        );
        assert_eq!(usage_of("com.walk"), walk_package("com.walk"));
        assert_eq!(walk_package("com.nothing"), 0);
        packages::set_running(None);
    }

    #[test]
    fn a_growth_past_the_cap_is_refused_and_a_shrink_never_is() {
        let _g = packages::test_support::lock();
        packages::set_running(Some("com.cap"));
        blob("/data/com.cap/seed", 10);
        let cap = APP_DATA_CAP_BYTES as u64;
        assert!(cap > 0, "the boardless layout has a cap");
        let room = available_for_app();
        assert_eq!(room, cap - (DIR_BYTES + BLOCK));
        assert_eq!(charge(room as i64), Ok(()));
        assert_eq!(charge(BLOCK as i64), Err(Refused::Cap));
        assert_eq!(available_for_app(), 0);
        assert_eq!(charge(-(BLOCK as i64)), Ok(()));
        assert_eq!(available_for_app(), BLOCK);
        packages::set_running(None);
    }

    #[test]
    fn a_growth_into_the_reserve_is_refused() {
        let _g = packages::test_support::lock();
        packages::set_running(Some("com.reserve"));
        let (total, free) = fs::space();
        assert!(total > 0 && free <= total);
        // Fill the test volume down into the reserve with another package's
        // data — big blobs, then single blocks — so the running app may not
        // take one more block. The fillers go before the asserts: the volume
        // is shared with every other storage test.
        let reserve = FS_SYSTEM_RESERVE_BYTES as u64;
        let mut i = 0;
        while fs::space().1 >= reserve + 16 * BLOCK {
            blob(&alloc::format!("/data/com.filler/{i}"), 16 * BLOCK as usize);
            i += 1;
        }
        while fs::space().1 >= reserve + BLOCK {
            blob(&alloc::format!("/data/com.filler/{i}"), BLOCK as usize);
            i += 1;
        }
        let refused = charge(BLOCK as i64);
        let room = available_for_app();
        for j in 0..i {
            fs::delete(&alloc::format!("/data/com.filler/{j}"));
        }
        packages::set_running(None);
        assert_eq!(refused, Err(Refused::Reserve));
        assert_eq!(room, 0);
    }
}
