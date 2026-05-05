// SPDX-License-Identifier: GPL-3.0-only
//! On-chip filesystem (LittleFS) living in the `FS_FLASH` linker region.
//!
//! Boot flow: [`init`] is called once, before the FreeRTOS scheduler starts,
//! so mount/format runs single-core with no cross-core XIP-disable hazard.
//! At the start of `start_tasks` (still pre-scheduler) [`worker::spawn`]
//! creates a dedicated core-0-pinned fs-worker task.  Every runtime caller
//! reaches the filesystem by invoking [`with_fs`], which packages the
//! closure and blocks until the worker has run it.
//!
//! Runtime concurrency: all filesystem mutation happens inside the worker,
//! so same-core interleaving between Java threads is impossible — each
//! request runs to completion before the next one is dequeued.  The worker's
//! core affinity keeps the flash XIP-disable window same-core as well.
//! Flash writes still disable interrupts during the ROM call, so the worker
//! task is stalled for the duration of each erase/program; that stall is
//! contained to one well-known task instead of being spread across callers.
//!
//! In simulator builds the same `Filesystem` runs against a host-file image
//! ([`storage_host::HostFileStorage`]) so Java File I/O persists across sim
//! runs; sim's `cell` uses an `std::sync::Mutex`, which already serialises
//! concurrent callers without needing a worker task.

use littlefs_rust::{Config, Filesystem};

pub mod error;
#[cfg(not(feature = "sim"))]
pub mod storage;
#[cfg(feature = "sim")]
pub mod storage_host;
#[cfg(all(not(feature = "sim"), feature = "family-rp"))]
pub mod worker;

pub use error::FsError;

#[cfg(not(feature = "sim"))]
pub use storage::FlashStorage as FsStorage;
#[cfg(feature = "sim")]
pub use storage_host::HostFileStorage as FsStorage;

#[cfg(not(feature = "sim"))]
mod layout {
    pub use super::storage::{BLOCK_SIZE, PROG_SIZE, READ_SIZE};
}
#[cfg(feature = "sim")]
mod layout {
    pub use super::storage_host::{BLOCK_SIZE, PROG_SIZE, READ_SIZE};
}

// ── FS singleton ───────────────────────────────────────────────────────────

#[cfg(not(feature = "sim"))]
mod cell {
    use super::*;
    use core::cell::UnsafeCell;

    pub(super) struct FsCell(UnsafeCell<Option<Filesystem<FsStorage>>>);
    // SAFETY: access is serialised by the single-threaded invariant
    // documented at the module level — `init` runs pre-scheduler, and
    // runtime callers are all on core 0.
    unsafe impl Sync for FsCell {}

    pub(super) static FS: FsCell = FsCell(UnsafeCell::new(None));

    pub(super) fn install(fs: Filesystem<FsStorage>) {
        // Safety: single-threaded init — see module-level SAFETY note.
        unsafe { *FS.0.get() = Some(fs) };
    }

    pub(super) fn with<R>(f: impl FnOnce(&Filesystem<FsStorage>) -> R) -> Option<R> {
        // Safety: see module-level SAFETY note.
        let slot = unsafe { &*FS.0.get() };
        slot.as_ref().map(f)
    }
}

#[cfg(feature = "sim")]
mod cell {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // Newtype so we can assert Send/Sync. `Filesystem` holds raw pointers
    // that don't auto-impl Send; the Mutex around this serialises all
    // access, and the raw pointers refer into heap owned by the same
    // filesystem instance, so moving the whole thing between threads under
    // the mutex is sound.
    struct FsBox(Filesystem<FsStorage>);
    unsafe impl Send for FsBox {}

    static FS: OnceLock<Mutex<Option<FsBox>>> = OnceLock::new();

    fn slot() -> &'static Mutex<Option<FsBox>> {
        FS.get_or_init(|| Mutex::new(None))
    }

    pub(super) fn install(fs: Filesystem<FsStorage>) {
        *slot().lock().unwrap() = Some(FsBox(fs));
    }

    pub(super) fn with<R>(f: impl FnOnce(&Filesystem<FsStorage>) -> R) -> Option<R> {
        slot().lock().unwrap().as_ref().map(|b| f(&b.0))
    }
}

fn config_for(storage: &FsStorage) -> Config {
    let mut cfg = Config::new(layout::BLOCK_SIZE as u32, storage.block_count());
    cfg.read_size = layout::READ_SIZE as u32;
    cfg.prog_size = layout::PROG_SIZE as u32;
    cfg.block_cycles = 500;
    cfg
}

/// Mount the filesystem, formatting on first boot or after corruption.
///
/// Must be called exactly once, before `FreeRtosUtils::start_scheduler`.
pub fn init() -> Result<(), FsError> {
    let mut storage = new_storage()?;
    let config = config_for(&storage);

    // Try to mount first; only format if mount reports corruption.
    let fs = match Filesystem::mount(storage, config) {
        Ok(fs) => fs,
        Err((FsError::Corrupt, recovered)) => {
            storage = recovered;
            let cfg = config_for(&storage);
            Filesystem::format(&mut storage, &cfg)?;
            Filesystem::mount(storage, cfg).map_err(|(e, _)| e)?
        }
        Err((e, _)) => return Err(e),
    };

    cell::install(fs);
    Ok(())
}

#[cfg(not(feature = "sim"))]
fn new_storage() -> Result<FsStorage, FsError> {
    Ok(FsStorage::new())
}

#[cfg(feature = "sim")]
fn new_storage() -> Result<FsStorage, FsError> {
    FsStorage::new().map_err(|_| FsError::Io)
}

/// Run `f` with exclusive access to the mounted filesystem.
///
/// On hardware this dispatches to the fs-worker task (see [`worker`]).  The
/// caller blocks until the worker has run the closure; on return, any output
/// captured by the closure has been written.  Must be called from a task
/// context — calling pre-scheduler will fail to obtain `Task::current()`.
///
/// On sim the closure runs synchronously under a mutex.
///
/// Returns `None` only if the mount failed in [`init`].
pub fn with_fs<R, F>(f: F) -> Option<R>
where
    F: FnOnce(&Filesystem<FsStorage>) -> R,
{
    #[cfg(all(not(feature = "sim"), feature = "family-rp"))]
    {
        Some(worker::submit(f))
    }
    // sim (host): run closure synchronously (single-threaded).
    // Note: sim builds use board-testbench-rp2350 which activates family-rp,
    // so we must gate on `sim` explicitly rather than `not(family-rp)`.
    #[cfg(feature = "sim")]
    {
        cell::with(f)
    }
}
