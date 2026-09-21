// SPDX-License-Identifier: GPL-3.0-only
//! The package directory: which apps are installed, where their runs sit in
//! the app region, where the next one goes, and how gaps are closed
//! (docs/designs/multi-app-2026-09.md D3–D6).
//!
//! The region is an allocator of 4 KB sectors. An installed app is a *run*:
//! a boot-meta sector (`papk_format::flash_image`) followed by its PAPK,
//! placed first-fit. There is no persisted table — [`rescan`] rebuilds the
//! directory by walking the region sector by sector: a sector whose header
//! parses starts a run and the walk skips past it, anything else steps one
//! sector. That is at most `region / 4 KB` header reads (384 on rp2350),
//! sub-millisecond from XIP, plus one manifest parse per run.
//!
//! An entry keeps the manifest's package name, version, label and icon as
//! slices into the image — read once when the entry is made, never copied —
//! so a lookup by name or a `PackageManager` query is a field read, not a
//! manifest parse from XIP flash. That is ~60 bytes an entry on a chip whose
//! image leaves ~18 KB of RAM.
//!
//! # Who writes
//!
//! One writer at a time: the boot path before the scheduler starts, the
//! debug bridge after an install or uninstall with the JVM parked, and the
//! simulator's control requests, serviced on the JVM task. Readers on the
//! JVM task (the boot image; in M2 `PackageManager`) never overlap a writer
//! for those reasons, which is what makes the plain static sound.

use core::cell::UnsafeCell;

use papk_format::flash_image::{
    is_committed, parse_header, FLAG_BOOT_DEFAULT, META_READ_LEN, META_SIZE,
};
use papk_format::Papk;

use crate::board_cfg::flash::BOOT_PACKAGE;
use crate::board_cfg::system_apks::{BOOT_APP, BOOT_LAUNCHER, LAUNCHER_PACKAGE};
use crate::install::PapkFlash;
// The installer owns the plan vocabulary; re-exported so the directory's API
// reads as it always has.
#[cfg(has_multi_app)]
use crate::install::PAGES_PER_SECTOR;
pub use crate::install::{run_sectors, Plan, PlanError};

pub use crate::board_cfg::flash::MAX_INSTALLED_APPS;

/// Run alignment and erase granularity: the boot-meta sector.
pub const SECTOR: usize = META_SIZE;
/// System apps (M2) the directory holds beside the installed ones.
pub const SYSTEM_MAX: usize = 2;
/// Slots in the directory: the installed apps and the system apps.
pub const CAPACITY: usize = MAX_INSTALLED_APPS + SYSTEM_MAX;
/// Stale runs a scan can remember for [`cleanup`]; more than this on one
/// boot would take several interrupted installs in a row.
const STALE_MAX: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A run in the app region.
    App,
    /// Linked into the firmware image (M2).
    System,
}

/// One installed package.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// The PAPK, in place: XIP flash on a device, the region buffer in the
    /// simulator, `.rodata` for a system app.
    pub image: &'static [u8],
    pub first_sector: u16,
    pub sectors: u16,
    pub flags: u32,
    pub seq: u32,
    pub kind: Kind,
    // The manifest, read once: slices into `image`, so they are right for
    // exactly as long as the entry is — a rescan makes new entries.
    name: &'static str,
    version: &'static str,
    version_code: u32,
    label: Option<&'static str>,
    icon: Option<&'static str>,
}

impl Entry {
    /// An entry for `image`, with its manifest read; `None` when the image
    /// is not a PAPK or names no package.
    fn new(
        image: &'static [u8],
        first_sector: u16,
        sectors: u16,
        flags: u32,
        seq: u32,
        kind: Kind,
    ) -> Option<Entry> {
        let papk = Papk::parse(image).ok()?;
        let name = papk.package_name()?;
        Some(Entry {
            image,
            first_sector,
            sectors,
            flags,
            seq,
            kind,
            name,
            version: papk.version().unwrap_or("?"),
            version_code: papk.version_code().unwrap_or(1),
            label: papk.label(),
            icon: papk.icon(),
        })
    }

    pub fn package(&self) -> &'static str {
        self.name
    }

    /// The display name; the package name when the manifest sets none.
    pub fn label(&self) -> &'static str {
        self.label.unwrap_or(self.name)
    }

    pub fn version(&self) -> &'static str {
        self.version
    }

    /// `version-code`, 1 when the manifest predates the key.
    pub fn version_code(&self) -> u32 {
        self.version_code
    }

    pub fn icon(&self) -> Option<&'static str> {
        self.icon
    }

    pub fn size(&self) -> usize {
        self.image.len()
    }

    pub fn is_boot_default(&self) -> bool {
        self.flags & FLAG_BOOT_DEFAULT != 0
    }

    /// The run `flash.sh --app` linked into the region: sector 0, sequence
    /// 0 (`build_support/papk.rs::embed_papk_flash_init`). An install never
    /// writes sequence 0, so nothing else looks like this.
    fn is_baked(&self) -> bool {
        self.first_sector == 0 && self.seq == 0
    }

    fn run(&self) -> (u32, u32) {
        (self.first_sector as u32, self.sectors as u32)
    }
}

/// A package name copied out of a manifest, bounded so the static stays
/// small; a longer name is cut at 64 bytes.
///
/// Visible to the crate because [`crate::alarms`] keeps one per alarm: an
/// owner recorded there is compared against [`running`], so it must be the
/// same bounded copy and not a shorter one that could truncate differently.
#[derive(Clone, Copy)]
pub(crate) struct Name([u8; 64], usize);

impl Name {
    pub(crate) const EMPTY: Name = Name([0; 64], 0);

    pub(crate) fn set(&mut self, name: Option<&str>) {
        let bytes = name.unwrap_or("").as_bytes();
        let n = bytes.len().min(self.0.len());
        self.0[..n].copy_from_slice(&bytes[..n]);
        self.1 = n;
    }

    pub(crate) fn get(&self) -> Option<&str> {
        if self.1 == 0 {
            return None;
        }
        core::str::from_utf8(&self.0[..self.1]).ok()
    }
}

struct Dir {
    entries: [Option<Entry>; CAPACITY],
    /// Runs the last scan found that are not installed apps — commit-less
    /// headers and the losers of duplicates — as `(first_sector, sectors)`.
    stale: [Option<(u32, u32)>; STALE_MAX],
    region: Option<(*const u8, usize)>,
    /// The package `run_app` is executing.
    running: Name,
    /// The package a cross-package `startActivity` asked for; [`next_image`]
    /// hands it to the supervisor once the current app has exited.
    #[cfg_attr(not(has_multi_app), allow(dead_code))]
    pending: Name,
    /// Times in a row the launcher exited with no app launched in between;
    /// the second one stops the restarts (D11, A2).
    #[cfg_attr(not(has_multi_app), allow(dead_code))]
    launcher_exits: u8,
}

struct DirCell(UnsafeCell<Dir>);
// SAFETY: single-writer discipline, see the module docs.
unsafe impl Sync for DirCell {}

static DIR: DirCell = DirCell(UnsafeCell::new(Dir {
    entries: [None; CAPACITY],
    stale: [None; STALE_MAX],
    region: None,
    running: Name::EMPTY,
    pending: Name::EMPTY,
    launcher_exits: 0,
}));

fn dir() -> &'static mut Dir {
    unsafe { &mut *DIR.0.get() }
}

// ── Scanning ────────────────────────────────────────────────────────────────

/// Rebuild the app entries from a flash the directory reaches through
/// [`PapkFlash`] — the trait's contract makes `mapped_base` readable
/// whenever no erase or program is in flight, which is the caller's to
/// ensure.
pub fn rescan_region(flash: &impl PapkFlash) {
    // SAFETY: `PapkFlash` is an unsafe trait whose implementors promise
    // `mapped_base` maps `region_len` readable bytes.
    unsafe { rescan(flash.mapped_base(), flash.region_len()) }
}

/// Rebuild the app entries by walking the region at `base` (`len` bytes).
/// System entries survive.
///
/// # Safety
/// `base` must map `len` readable bytes for the life of the program (the
/// entries keep slices into it), and no erase or program of the region may
/// be in flight.
pub unsafe fn rescan(base: *const u8, len: usize) {
    let d = dir();
    d.region = Some((base, len));
    for e in d.entries.iter_mut() {
        if matches!(
            e,
            Some(Entry {
                kind: Kind::App,
                ..
            })
        ) {
            *e = None;
        }
    }
    d.stale = [None; STALE_MAX];

    let sectors = len / SECTOR;
    let mut s = 0usize;
    while s < sectors {
        // SAFETY: `base` maps `len` readable bytes (the caller's contract) and
        // `s` stays inside them.
        let sector_ptr = unsafe { base.add(s * SECTOR) };
        let header = unsafe { core::slice::from_raw_parts(sector_ptr, META_READ_LEN) };
        let max_len = (sectors - s - 1) * SECTOR;
        let Some(meta) = parse_header(header, max_len) else {
            s += 1;
            continue;
        };
        let span = run_sectors(meta.len as usize) as usize;
        if !is_committed(header) {
            // An install or relocation that never finished: not an app, not
            // free space either until `cleanup` erases it.
            note_stale(d, s as u32, span as u32);
            s += span;
            continue;
        }
        let image =
            unsafe { core::slice::from_raw_parts(sector_ptr.add(META_SIZE), meta.len as usize) };
        let entry = if papk_format::validate_structure(image).is_ok() {
            Entry::new(
                image,
                s as u16,
                span as u16,
                meta.flags,
                meta.seq,
                Kind::App,
            )
        } else {
            None
        };
        let Some(entry) = entry else {
            // A header over garbage: step past the header only, so a real
            // run that happens to start inside the claimed span is still found.
            s += 1;
            continue;
        };
        insert(d, entry);
        s += span;
    }
    log_directory(d);
    bump_directory_generation();
}

fn note_stale(d: &mut Dir, first: u32, sectors: u32) {
    if let Some(slot) = d.stale.iter_mut().find(|s| s.is_none()) {
        *slot = Some((first, sectors));
    }
}

/// Add an app entry, resolving a duplicate package and noting the loser
/// for cleanup. The baked run wins: a fresh `flash.sh --app` must run what
/// was just flashed, not an older copy a `pdb install` left behind (which
/// may not even be built for this firmware's map version). Otherwise the
/// higher `seq` wins, tie to the lower sector.
fn insert(d: &mut Dir, entry: Entry) {
    let package = entry.package();
    if d.entries
        .iter()
        .flatten()
        .any(|e| e.kind == Kind::System && e.package() == package)
    {
        // A run that names a system app cannot be installed (D5) and would
        // shadow the firmware's copy. It still occupies its sectors, so it
        // is stale rather than ignored: cleanup erases it.
        crate::pd_warn!(
            "[packages] run at sector {}: {} is a system app; erasing",
            entry.first_sector,
            package
        );
        note_stale(d, entry.first_sector as u32, entry.sectors as u32);
        return;
    }
    let existing = d
        .entries
        .iter()
        .position(|e| matches!(e, Some(e) if e.kind == Kind::App && e.package() == package));
    if let Some(i) = existing {
        let existing = d.entries[i].unwrap();
        let newer = if entry.is_baked() != existing.is_baked() {
            entry.is_baked()
        } else {
            entry.seq > existing.seq
                || (entry.seq == existing.seq && entry.first_sector < existing.first_sector)
        };
        let loser = if newer { existing } else { entry };
        note_stale(d, loser.first_sector as u32, loser.sectors as u32);
        if newer {
            d.entries[i] = Some(entry);
        }
        return;
    }
    match d.entries.iter().position(|e| e.is_none()) {
        Some(i) => d.entries[i] = Some(entry),
        None => crate::pd_warn!(
            "[packages] directory full: run at sector {} ignored",
            entry.first_sector
        ),
    }
}

/// Log the runs a scan found. System entries are logged once, by
/// [`register_system`], not on every rescan.
fn log_directory(d: &Dir) {
    for e in d.entries.iter().flatten() {
        if e.kind == Kind::App {
            crate::pd_info!(
                "[packages] sector {}: {} {} ({}) {} bytes{}",
                e.first_sector,
                e.package(),
                e.version(),
                e.version_code(),
                e.size(),
                if e.is_boot_default() { " [boot]" } else { "" }
            );
        }
    }
}

fn log_system(e: &Entry) {
    crate::pd_info!(
        "[packages] system: {} {} ({}) {} bytes",
        e.package(),
        e.version(),
        e.version_code(),
        e.size()
    );
}

/// Erase what the last scan found stale, then rescan. Runs where a
/// relocation or install can be trusted not to be in flight — at boot
/// before the scheduler, or with the JVM parked.
pub fn cleanup(flash: &mut impl PapkFlash) {
    let d = dir();
    let stale = d.stale;
    let mut erased = false;
    for (first, sectors) in stale.into_iter().flatten() {
        crate::pd_warn!(
            "[packages] erasing stale run at sector {} ({} sectors)",
            first,
            sectors
        );
        // SAFETY: the caller's contract — nothing executes from the region.
        unsafe { flash.erase_run(first, sectors) };
        erased = true;
    }
    if erased {
        rescan_region(flash);
    }
}

// ── System apps ─────────────────────────────────────────────────────────────

/// Add the system apps linked into the firmware (`board_cfg::system_apks`).
/// Runs once at boot, before the first scan; a rescan keeps these entries.
/// An image that is not a valid PAPK, has no `package-name`, was built for
/// another framework-map-version, repeats a package, or does not fit the
/// [`SYSTEM_MAX`] slots is skipped with a warning.
pub fn register_system(images: &[&'static [u8]]) {
    let d = dir();
    for &image in images {
        let papk = match Papk::parse(image) {
            Ok(p) if papk_format::validate_structure(image).is_ok() => p,
            _ => {
                crate::pd_warn!("[packages] system app skipped: not a valid PAPK");
                continue;
            }
        };
        let Some(package) = papk.package_name().filter(|p| !p.is_empty()) else {
            crate::pd_warn!("[packages] system app skipped: no package-name");
            continue;
        };
        if papk
            .verify_compat(crate::framework_map::FRAMEWORK_MAP_VERSION)
            .is_err()
        {
            crate::pd_warn!(
                "[packages] system app {} skipped: built for another framework-map-version",
                package
            );
            continue;
        }
        let systems = d
            .entries
            .iter()
            .flatten()
            .filter(|e| e.kind == Kind::System);
        if systems.clone().any(|e| e.package() == package) {
            crate::pd_warn!("[packages] system app {} listed twice; skipped", package);
            continue;
        }
        if systems.count() >= SYSTEM_MAX {
            crate::pd_warn!(
                "[packages] system app {} skipped: only {} fit",
                package,
                SYSTEM_MAX
            );
            continue;
        }
        let Some(entry) = Entry::new(image, 0, 0, 0, 0, Kind::System) else {
            continue; // parsed above: unreachable
        };
        match d.entries.iter().position(|e| e.is_none()) {
            Some(i) => {
                d.entries[i] = Some(entry);
                log_system(&entry);
            }
            None => crate::pd_warn!("[packages] directory full: system app {} skipped", package),
        }
    }
    bump_directory_generation();
}

/// The launcher: the system app named `LAUNCHER_PACKAGE`, when linked in.
pub fn launcher() -> Option<&'static Entry> {
    entries().find(|e| e.kind == Kind::System && e.package() == LAUNCHER_PACKAGE)
}

// ── Queries ─────────────────────────────────────────────────────────────────

pub fn entries() -> impl Iterator<Item = &'static Entry> {
    dir().entries.iter().flatten()
}

/// Installed apps, in no particular order.
pub fn apps() -> impl Iterator<Item = &'static Entry> {
    entries().filter(|e| e.kind == Kind::App)
}

pub fn find(package: &str) -> Option<&'static Entry> {
    entries().find(|e| e.package() == package)
}

pub fn is_system(package: &str) -> bool {
    matches!(find(package), Some(e) if e.kind == Kind::System)
}

/// The slot `package` occupies in the directory's array — what a per-slot
/// cache indexes. A rescan repacks the array, so a slot is only good for
/// one [`directory_generation`].
#[cfg(has_multi_app)]
pub fn slot_of(package: &str) -> Option<usize> {
    dir()
        .entries
        .iter()
        .position(|e| matches!(e, Some(e) if e.package() == package))
}

pub fn installed_count() -> u32 {
    apps().count() as u32
}

/// Sectors in the region, or 0 before the first scan.
pub fn region_sectors() -> u32 {
    dir()
        .region
        .map(|(_, len)| (len / SECTOR) as u32)
        .unwrap_or(0)
}

/// Free sectors: the largest contiguous gap and the total.
pub fn free_space() -> (u32, u32) {
    space(None)
}

/// The run flagged `BOOT_DEFAULT` (the lowest sector if several).
fn boot_default_run() -> Option<&'static Entry> {
    apps()
        .filter(|e| e.is_boot_default())
        .min_by_key(|e| e.first_sector)
}

fn lowest_run() -> Option<&'static Entry> {
    apps().min_by_key(|e| e.first_sector)
}

/// Which package boots (D7), in this order: `override_` (`flash.sh --boot`:
/// `app` is the baked run, `launcher` the launcher, anything else a package
/// name), the board's `boot_package`, the run flagged `BOOT_DEFAULT`, the
/// launcher, the run at the lowest sector. A name that is not installed is
/// skipped with a warning and the next rule applies.
pub fn select_boot(override_: Option<&str>, board: Option<&str>) -> Option<&'static Entry> {
    if let Some(want) = override_ {
        let found = match want {
            w if w == BOOT_APP => boot_default_run().or_else(lowest_run),
            w if w == BOOT_LAUNCHER => launcher(),
            package => find(package),
        };
        match found {
            Some(e) => {
                crate::pd_info!("[packages] boot: {} (--boot {})", e.package(), want);
                return Some(e);
            }
            None => crate::pd_warn!("[packages] --boot {}: not installed", want),
        }
    }
    if let Some(package) = board {
        match find(package) {
            Some(e) => {
                crate::pd_info!("[packages] boot: {} (boot_package)", e.package());
                return Some(e);
            }
            None => crate::pd_warn!("[packages] boot_package {}: not installed", package),
        }
    }
    let (e, rule) = if let Some(e) = boot_default_run() {
        (e, "boot default")
    } else if let Some(e) = launcher() {
        (e, "launcher")
    } else {
        (lowest_run()?, "lowest sector")
    };
    crate::pd_info!("[packages] boot: {} ({})", e.package(), rule);
    Some(e)
}

/// The image to boot: [`select_boot`] with the build-time override and the
/// board's `boot_package`.
pub fn boot_image() -> Option<&'static [u8]> {
    select_boot(boot_override(), BOOT_PACKAGE).map(|e| e.image)
}

/// `flash.sh --boot`: a build-time constant on a device. The simulator reads
/// `PICODROID_BOOT` when it starts, so changing it needs no rebuild.
fn boot_override() -> Option<&'static str> {
    #[cfg(feature = "sim")]
    {
        static OVERRIDE: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
        *OVERRIDE.get_or_init(|| {
            std::env::var("PICODROID_BOOT")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|s| &*alloc::boxed::Box::leak(s.into_boxed_str()))
        })
    }
    #[cfg(not(feature = "sim"))]
    {
        crate::board_cfg::system_apks::BOOT_OVERRIDE
    }
}

/// One more than the highest sequence number on the device.
pub fn next_seq() -> u32 {
    apps().map(|e| e.seq).max().unwrap_or(0).wrapping_add(1)
}

/// Record the package `run_app` is executing (its manifest's name, copied).
pub fn set_running(package: Option<&str>) {
    dir().running.set(package);
    // A load and a store rather than `fetch_add`: the Cortex-M0+ has no
    // read-modify-write atomics, and this has one writer (the JVM task).
    let next = RUN_GENERATION
        .load(core::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1);
    RUN_GENERATION.store(next, core::sync::atomic::Ordering::Release);
}

/// Moves with every [`set_running`]: what a per-run cache keys on, such as
/// the storage sandbox's "package directory made" flag.
static RUN_GENERATION: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// The current run's number; another value means another `run_app`.
pub fn run_generation() -> u32 {
    RUN_GENERATION.load(core::sync::atomic::Ordering::Acquire)
}

/// Moves with every change to the slots — a rescan, a system registration —
/// so a cache indexed by slot knows when its rows may point elsewhere.
static DIRECTORY_GENERATION: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// The directory's number; another value means the slots were rebuilt.
pub fn directory_generation() -> u32 {
    DIRECTORY_GENERATION.load(core::sync::atomic::Ordering::Acquire)
}

fn bump_directory_generation() {
    // A load and a store, as `set_running`: one writer, no RMW atomics.
    let next = DIRECTORY_GENERATION
        .load(core::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1);
    DIRECTORY_GENERATION.store(next, core::sync::atomic::Ordering::Release);
}

/// The package `run_app` is executing, if it named one.
pub fn running() -> Option<&'static str> {
    dir().running.get()
}

// ── Switching (multi-app boards) ────────────────────────────────────────────

/// A `startActivity` named a package that is not installed.
#[cfg(has_multi_app)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotFound;

/// Ask the supervisor to run `package` next: the lifecycle loop tears the
/// current app down, then [`next_image`] hands out this package's image.
/// The name is kept rather than the image, so an install of the same
/// package in between (the simulator reinstalling the running app) launches
/// the new copy.
#[cfg(has_multi_app)]
pub fn request_launch(package: &str) -> Result<(), NotFound> {
    if find(package).is_none() {
        return Err(NotFound);
    }
    dir().pending.set(Some(package));
    Ok(())
}

/// Ask the supervisor to go back to the launcher, as the HOME key does.
///
/// `false` when there is nothing to go home to — no launcher is linked in, or
/// the launcher is already what is running — and the caller should then leave
/// the current app alone rather than tearing it down for nothing.
#[cfg(has_multi_app)]
pub fn request_home() -> bool {
    let Some(package) = launcher().map(|l| l.package()) else {
        return false;
    };
    if running() == Some(package) {
        return false;
    }
    request_launch(package).is_ok()
}

/// A single-app board has no launcher, so HOME has nowhere to go.
#[cfg(not(has_multi_app))]
pub fn request_home() -> bool {
    false
}

/// What runs after `run_app` returns (D11, A2): the pending launch; else the
/// launcher, when the app that exited was not it; else nothing, and the
/// supervisor waits for an install as a single-app board does. A launcher
/// that exits is started again once; a second exit in a row is a fault,
/// and the device waits for an install instead of looping.
#[cfg(has_multi_app)]
pub fn next_image() -> Option<&'static [u8]> {
    let d = dir();
    let pending = d.pending;
    d.pending = Name::EMPTY;
    if let Some(package) = pending.get() {
        match find(package) {
            Some(e) => {
                d.launcher_exits = 0;
                return Some(e.image);
            }
            None => crate::pd_warn!("[packages] launch of {} dropped: not installed", package),
        }
    }
    let l = launcher()?;
    if running() == Some(l.package()) {
        d.launcher_exits = d.launcher_exits.saturating_add(1);
        if d.launcher_exits >= 2 {
            crate::pd_warn!("[packages] launcher exited twice; waiting for an install");
            return None;
        }
        crate::pd_warn!("[packages] launcher exited; starting it again");
    } else {
        d.launcher_exits = 0;
    }
    Some(l.image)
}

/// A single-app board never switches: the supervisor waits for an install.
#[cfg(not(has_multi_app))]
pub fn next_image() -> Option<&'static [u8]> {
    None
}

// ── Uninstall from Java (multi-app boards, M3c) ─────────────────────────────

/// What `PackageInstaller.uninstall` reports back to Java.
#[cfg(has_multi_app)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UninstallOutcome {
    Done = 0,
    NotInstalled = 1,
    System = 2,
    Running = 3,
    Failed = 4,
}

/// The run `package` occupies, if Java may uninstall it: an installed app
/// that is neither a system app nor the one asking.
#[cfg(has_multi_app)]
pub fn uninstall_target(package: &str) -> Result<(u32, u32), UninstallOutcome> {
    let entry = find(package).ok_or(UninstallOutcome::NotInstalled)?;
    if entry.kind == Kind::System {
        return Err(UninstallOutcome::System);
    }
    if running() == Some(package) {
        return Err(UninstallOutcome::Running);
    }
    Ok((u32::from(entry.first_sector), u32::from(entry.sectors)))
}

/// Java's `PackageInstaller.uninstall`: the checks, the erase through the
/// platform (`host::uninstall_run`, which rescans too), then the data.
#[cfg(has_multi_app)]
pub fn uninstall_from_app(package: &str) -> UninstallOutcome {
    let (first, sectors) = match uninstall_target(package) {
        Ok(run) => run,
        Err(outcome) => return outcome,
    };
    if !crate::host::uninstall_run(first, sectors) {
        return UninstallOutcome::Failed;
    }
    crate::storage::wipe_package(package);
    crate::pd_info!("[packages] {} uninstalled from Java", package);
    UninstallOutcome::Done
}

/// Forget everything: the directory is a process-wide static and every test
/// starts from an empty one.
#[cfg(test)]
pub(crate) fn reset_for_test() {
    let d = dir();
    d.entries = [None; CAPACITY];
    d.stale = [None; STALE_MAX];
    d.region = None;
    d.running = Name::EMPTY;
    d.pending = Name::EMPTY;
    d.launcher_exits = 0;
    bump_directory_generation();
}

// Placement and compaction.
mod plan;
pub use self::plan::*;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
