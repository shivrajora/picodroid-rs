// SPDX-License-Identifier: GPL-3.0-only
//! LittleFS, mounted once and reached through a serial worker.
//!
//! Compiled only under `feature = "littlefs"`, which is **off by default**.
//! That is the point of the feature rather than a hedge: [`crate::hal::HalFs`]
//! remains the only contract this crate imposes, so a family backed by FAT, by
//! NVS, or by nothing implements that trait and never links a byte of this.
//! What a family that *does* want LittleFS gets is the ~400 lines below —
//! mount recovery, the singleton, the host image, and the worker discipline —
//! rather than a fresh copy of each.
//!
//! # Boot flow
//!
//! [`init_device`] (or [`init_host_image`]) runs once, before the scheduler
//! starts, so mount and format happen single-threaded with no cross-core
//! XIP-disable hazard. [`spawn_worker`] then creates the worker task, still
//! pre-scheduler. Every runtime caller goes through [`with_fs`].
//!
//! # Runtime concurrency
//!
//! All filesystem access happens inside the worker, so interleaving between
//! Java threads is impossible by construction — each request runs to
//! completion before the next is dequeued. The worker's core affinity, which
//! the platform attaches to [`TaskKind::FsWorker`], keeps the XIP-disable
//! window on one core too. Flash writes still disable interrupts for the
//! duration of the ROM call; that stall now lands on one well-known task
//! instead of on whichever caller happened to ask.
//!
//! The simulator runs the same topology against a host-file image, because it
//! runs the same kernel (`docs/designs/freertos-host-sim.md` §1.2).
//!
//! # Why the backing store is boxed
//!
//! The mounted filesystem is a `static`, and statics cannot be generic — so
//! the concrete storage type has to stop being a type parameter somewhere.
//! It stops here: [`init_device`] takes any [`FsBackingStore`] and boxes it
//! into [`DynStorage`], leaving one concrete [`Fs`] for device and host alike.
//! The `dyn` is *inside* this crate, not on a `__pd_*` registration seam, and
//! it costs one vtable hop per block operation against millisecond-scale
//! flash (`docs/designs/family-neutral-residue.md` D2).

use littlefs_rust::{Config, Error as LfsError, Filesystem, Storage as LfsStorage};

use crate::executors::serial_worker::SerialWorker;
use crate::rtos::TaskKind;

pub mod hal_impl;
#[cfg(feature = "sim")]
pub mod storage_host;

pub use hal_impl::LittleFsHal;
pub use littlefs_rust::Error as FsError;

/// The block geometry a backing store reports.
///
/// Defaults match the RP family's NOR flash and the host image that mirrors
/// it byte for byte. A store whose device differs overrides
/// [`FsBackingStore::geometry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FsGeometry {
    pub block: u32,
    pub prog: u32,
    pub read: u32,
}

impl Default for FsGeometry {
    fn default() -> Self {
        Self {
            block: 4096,
            prog: 256,
            read: 16,
        }
    }
}

/// A block device LittleFS can be mounted on.
///
/// Object-safe on purpose — see the module docs on boxing.
pub trait FsBackingStore: LfsStorage {
    fn block_count(&self) -> u32;
    fn geometry(&self) -> FsGeometry {
        FsGeometry::default()
    }
}

/// The boxed backing store the mounted filesystem actually talks to.
pub struct DynStorage(alloc::boxed::Box<dyn FsBackingStore + Send>);

impl LfsStorage for DynStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        self.0.read(block, offset, buf)
    }
    fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
        self.0.write(block, offset, data)
    }
    fn erase(&mut self, block: u32) -> Result<(), LfsError> {
        self.0.erase(block)
    }
}

/// The mounted filesystem, as callers see it.
///
/// An alias so no caller has to name [`DynStorage`], which is an artefact of
/// the static and not something a closure should have to know about.
pub type Fs = Filesystem<DynStorage>;

// ── FS singleton ───────────────────────────────────────────────────────────

mod cell {
    use super::Fs;
    use core::cell::UnsafeCell;

    pub(super) struct FsCell(UnsafeCell<Option<Fs>>);
    // SAFETY: `install` runs pre-scheduler, single-threaded; every runtime
    // access goes through the fs worker task, and only that task calls `with`.
    unsafe impl Sync for FsCell {}

    pub(super) static FS: FsCell = FsCell(UnsafeCell::new(None));

    pub(super) fn install(fs: Fs) {
        // SAFETY: single-threaded init — see the SAFETY note above.
        unsafe { *FS.0.get() = Some(fs) };
    }

    pub(super) fn with<R>(f: impl FnOnce(&Fs) -> R) -> Option<R> {
        // SAFETY: see above.
        let slot = unsafe { &*FS.0.get() };
        slot.as_ref().map(f)
    }
}

static WORKER: SerialWorker = SerialWorker::new();

fn config_for(geometry: FsGeometry, block_count: u32) -> Config {
    let mut cfg = Config::new(geometry.block, block_count);
    cfg.read_size = geometry.read;
    cfg.prog_size = geometry.prog;
    cfg.block_cycles = 500;
    cfg
}

/// Mount `storage`, formatting on first boot or after corruption.
///
/// Must be called exactly once, before the scheduler starts.
pub fn init_device(storage: impl FsBackingStore + Send + 'static) -> Result<(), FsError> {
    mount(DynStorage(alloc::boxed::Box::new(storage)))
}

/// Mount the simulator's host-file image. See [`storage_host`].
#[cfg(feature = "sim")]
pub fn init_host_image() -> Result<(), FsError> {
    let storage = storage_host::HostFileStorage::new().map_err(|_| FsError::Io)?;
    init_device(storage)
}

fn mount(mut storage: DynStorage) -> Result<(), FsError> {
    let geometry = storage.0.geometry();
    let block_count = storage.0.block_count();
    let config = config_for(geometry, block_count);

    // Mount first; format only if the mount reports corruption. The order
    // matters: formatting unconditionally would erase a working filesystem on
    // every boot, and the failure would look like "persistence is broken"
    // rather than like a bug here.
    let fs = match Filesystem::mount(storage, config) {
        Ok(fs) => fs,
        Err((FsError::Corrupt, recovered)) => {
            storage = recovered;
            let cfg = config_for(geometry, block_count);
            Filesystem::format(&mut storage, &cfg)?;
            Filesystem::mount(storage, cfg).map_err(|(e, _)| e)?
        }
        Err((e, _)) => return Err(e),
    };

    cell::install(fs);
    Ok(())
}

/// Create the filesystem worker task.
///
/// Must be called after [`init_device`] and before the scheduler starts. The
/// platform decides this task's stack size, priority band and core affinity
/// from [`TaskKind::FsWorker`].
pub fn spawn_worker() {
    WORKER.spawn(
        "fs",
        TaskKind::FsWorker,
        crate::task_priority::PRIORITY_FS_WORKER,
    );
}

/// Run `f` with exclusive access to the mounted filesystem.
///
/// Dispatches to the worker task; the caller blocks until it has run the
/// closure. Returns `None` only if the mount failed at init.
///
/// Before the scheduler is running there is no task to submit from and no
/// concurrency to serialise, so the closure runs inline. That arm covers boot
/// and is the only correct option there, not an optimisation.
///
/// The test is `scheduler_running`, not `task_current() != 0`: FreeRTOS makes
/// the first *created* task current, so during boot `task_current` returns a
/// live handle for a task that cannot run, and submitting to it would block
/// forever.
pub fn with_fs<R, F>(f: F) -> Option<R>
where
    F: FnOnce(&Fs) -> R,
{
    if !crate::rtos::scheduler_running() {
        return cell::with(f);
    }
    WORKER.submit(move || cell::with(f))
}
