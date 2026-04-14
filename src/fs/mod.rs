//! On-chip filesystem (LittleFS) living in the `FS_FLASH` linker region.
//!
//! Boot flow: [`init`] is called once, before the FreeRTOS scheduler starts,
//! so mount/format runs single-core with no cross-core XIP-disable hazard.
//! Callers reach the mounted filesystem through [`with_fs`].
//!
//! Runtime SMP safety: flash writes disable XIP for the duration of the ROM
//! call.  Today all callers happen on core 0 and are serialised by the
//! ambient task structure — there is no cross-core concurrent write path.
//! When a second writer is added (e.g. a future PDB-driven install into the
//! FS region) it must hook into `park_for_flash` the same way PAPK installs
//! already do.

use core::cell::UnsafeCell;

use littlefs_rust::{Config, Filesystem};

pub mod error;
pub mod storage;

pub use error::FsError;
pub use storage::FlashStorage;

struct FsCell(UnsafeCell<Option<Filesystem<FlashStorage>>>);

// SAFETY: access is serialised by the single-threaded invariant documented
// above — `init` runs pre-scheduler, and runtime callers are all on core 0.
unsafe impl Sync for FsCell {}

static FS: FsCell = FsCell(UnsafeCell::new(None));

fn config_for(storage: &FlashStorage) -> Config {
    let mut cfg = Config::new(storage::BLOCK_SIZE as u32, storage.block_count());
    cfg.read_size = storage::READ_SIZE as u32;
    cfg.prog_size = storage::PROG_SIZE as u32;
    cfg.block_cycles = 500;
    cfg
}

/// Mount the filesystem, formatting on first boot or after corruption.
///
/// Must be called exactly once, before `FreeRtosUtils::start_scheduler`.
pub fn init() -> Result<(), FsError> {
    let mut storage = FlashStorage::new();
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

    // Safety: single-threaded init — see module-level SAFETY note.
    unsafe { *FS.0.get() = Some(fs) };
    Ok(())
}

/// Run `f` with exclusive access to the mounted filesystem.  Returns `None`
/// if [`init`] has not been called or the previous mount failed.
pub fn with_fs<R>(f: impl FnOnce(&Filesystem<FlashStorage>) -> R) -> Option<R> {
    // Safety: see module-level SAFETY note.  Callers must not hold the
    // returned borrow across task-switch points; the closure form enforces
    // lexical scoping.
    let slot = unsafe { &*FS.0.get() };
    slot.as_ref().map(f)
}
