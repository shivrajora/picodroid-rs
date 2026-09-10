// SPDX-License-Identifier: GPL-3.0-only
//! Per-app storage on the one LittleFS volume
//! (docs/designs/multi-app-2026-09.md D10, §8): the sandbox that confines
//! every app path to `/data/<package>`, the quota that bounds what it may
//! hold, the wipe an uninstall runs and the sweep a boot runs. All of it
//! reaches storage through `crate::hal::fs`, so it is family-neutral and
//! runs under test over the in-memory `TestHal`.

pub mod quota;
pub mod sandbox;

use alloc::vec::Vec;

use crate::hal::fs;

/// How deep a wipe follows subdirectories — the quota walks no deeper.
const WIPE_DEPTH: usize = 4;

/// Remove `/data/<package>` and everything under it: what an uninstall
/// does with the package's data (D10, P8). `true` when nothing is left.
pub fn wipe_package(package: &str) -> bool {
    let mut buf = [0u8; sandbox::BUF];
    let Ok(root) = sandbox::resolve(Some(package), "", &mut buf) else {
        return false;
    };
    let gone = remove_tree(root, WIPE_DEPTH);
    quota::forget(package);
    gone
}

/// Delete `dir`'s files, its subdirectories (deepest first), then `dir`.
fn remove_tree(dir: &str, depth: usize) -> bool {
    let mut entries = Vec::new();
    if !fs::list_dir(dir, &mut entries) {
        // Not a directory: a file of that name, or nothing.
        return !fs::exists(dir) || fs::delete(dir);
    }
    let mut ok = true;
    for entry in entries {
        let child = alloc::format!("{dir}/{}", entry.name);
        ok &= if entry.dir {
            depth > 0 && remove_tree(&child, depth - 1)
        } else {
            fs::delete(&child)
        };
    }
    // The directory itself: gone once deleted, or once nothing answers to
    // its name (a HAL that keeps no directory entries of its own).
    let dir_gone = fs::delete(dir) || !fs::exists(dir);
    ok && dir_gone
}

/// Remove every `/data/*` directory that names no installed or system
/// package — what a power loss between an uninstall's erase and its wipe,
/// or an app removed by reflashing, leaves behind. Multi-app boards, at
/// boot, once the package directory is scanned. Returns how many went.
#[cfg(has_multi_app)]
pub fn sweep_orphans() -> usize {
    let mut entries = Vec::new();
    if !fs::list_dir(sandbox::DATA_ROOT, &mut entries) {
        return 0;
    }
    let mut removed = 0;
    for entry in entries {
        if entry.dir && crate::packages::find(&entry.name).is_none() {
            if wipe_package(&entry.name) {
                crate::pd_info!(
                    "[storage] swept the data of {}, not installed",
                    entry.name.as_str()
                );
                removed += 1;
            } else {
                crate::pd_warn!(
                    "[storage] could not sweep the data of {}",
                    entry.name.as_str()
                );
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages;

    #[test]
    fn a_wipe_removes_the_tree_and_a_missing_directory_is_fine() {
        let _g = packages::test_support::lock();
        fs::truncate("/data/com.wipe/a");
        fs::truncate("/data/com.wipe/sub/deeper/b");
        assert!(wipe_package("com.wipe"));
        assert!(!fs::exists("/data/com.wipe/a"));
        assert!(!fs::exists("/data/com.wipe/sub/deeper/b"));
        assert!(wipe_package("com.never"));
    }

    #[cfg(has_multi_app)]
    #[test]
    fn the_sweep_removes_what_no_package_owns() {
        let _g = packages::test_support::lock();
        packages::reset_for_test();
        fs::truncate("/data/com.orphan/x");
        fs::truncate("/data/com.orphan2/y/z");
        let before = sweep_orphans();
        assert!(before >= 2, "{before}");
        assert!(!fs::exists("/data/com.orphan/x"));
        assert!(!fs::exists("/data/com.orphan2/y/z"));
        assert_eq!(sweep_orphans(), 0);
    }
}
