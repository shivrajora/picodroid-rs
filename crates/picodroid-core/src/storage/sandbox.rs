// SPDX-License-Identifier: GPL-3.0-only
//! The storage sandbox: every path an app names is confined to its own
//! directory (docs/designs/multi-app-2026-09.md D10, §8 P1).
//!
//! The path map itself — `/x/y` to `/data/<package>/x/y`, `..` refused, the
//! [`BUF`]-byte buffer and the [`APP_PATH_MAX`] cap — is `pd_codec::sandbox`,
//! re-exported here, so the host fuzzes it (`sandbox` target) without the
//! platform behind it. This module supplies the running package
//! (`packages::running()`, through `native_handler::io::mapped`) and
//! creates its directory.

use core::sync::atomic::{AtomicU32, Ordering};

pub use pd_codec::sandbox::{resolve, Rejected, APP_PATH_MAX, BUF, DATA_ROOT, PACKAGE_MAX};

use crate::packages;

/// Create `/data/<package>` ahead of an app's first creating operation of a
/// run. `packages::run_generation()` moves when `run_app` records the next
/// package and the flag follows it, so each run pays two `mkdir`s once; two
/// Java threads racing here both `mkdir`, which is idempotent.
pub fn ensure_package_dir() {
    static ENSURED_FOR: AtomicU32 = AtomicU32::new(u32::MAX);
    let generation = packages::run_generation();
    if ENSURED_FOR.load(Ordering::Acquire) == generation {
        return;
    }
    if let Some(package) = packages::running() {
        let mut buf = [0u8; BUF];
        if let Ok(root) = resolve(Some(package), "", &mut buf) {
            let _ = crate::hal::fs::mkdir(DATA_ROOT);
            let _ = crate::hal::fs::mkdir(root);
            // The quota may have walked the package before its directory
            // existed; the pair it just gained must be counted.
            crate::storage::quota::invalidate();
        }
    }
    ENSURED_FOR.store(generation, Ordering::Release);
}
