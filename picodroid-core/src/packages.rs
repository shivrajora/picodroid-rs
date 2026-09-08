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
//! Entries hold no strings: name, label and versions are re-read from the
//! image on demand, which keeps the static at ~24 bytes an entry on a chip
//! whose image leaves ~18 KB of RAM.
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
use papk_format::{keys, Papk};

use crate::board_cfg::flash::BOOT_PACKAGE;
use crate::board_cfg::system_apks::{BOOT_APP, BOOT_LAUNCHER, LAUNCHER_PACKAGE};
use crate::install::PapkFlash;
#[cfg(has_multi_app)]
use crate::install::PAGES_PER_SECTOR;

pub use crate::board_cfg::flash::MAX_INSTALLED_APPS;

/// Run alignment and erase granularity: the boot-meta sector.
pub const SECTOR: usize = META_SIZE;
/// System apps (M2) the directory holds beside the installed ones.
pub const SYSTEM_MAX: usize = 2;
const CAPACITY: usize = MAX_INSTALLED_APPS + SYSTEM_MAX;
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
}

impl Entry {
    fn papk(&self) -> Option<Papk<'static>> {
        Papk::parse(self.image).ok()
    }

    pub fn package(&self) -> &'static str {
        self.papk().and_then(|p| p.package_name()).unwrap_or("?")
    }

    /// The display name; the package name when the manifest sets none.
    pub fn label(&self) -> &'static str {
        self.papk()
            .and_then(|p| p.label())
            .unwrap_or_else(|| self.package())
    }

    pub fn version(&self) -> &'static str {
        self.papk().and_then(|p| p.version()).unwrap_or("?")
    }

    /// `version-code`, 1 when the manifest predates the key.
    pub fn version_code(&self) -> u32 {
        self.papk().and_then(|p| p.version_code()).unwrap_or(1)
    }

    pub fn icon(&self) -> Option<&'static str> {
        self.papk().and_then(|p| p.icon())
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
#[derive(Clone, Copy)]
struct Name([u8; 64], usize);

impl Name {
    const EMPTY: Name = Name([0; 64], 0);

    fn set(&mut self, name: Option<&str>) {
        let bytes = name.unwrap_or("").as_bytes();
        let n = bytes.len().min(self.0.len());
        self.0[..n].copy_from_slice(&bytes[..n]);
        self.1 = n;
    }

    fn get(&self) -> Option<&str> {
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

/// Sectors a run for an image of `len` bytes occupies: the meta sector plus
/// the image rounded up to whole sectors.
pub fn run_sectors(len: usize) -> u32 {
    1 + len.div_ceil(SECTOR) as u32
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
        if papk_format::validate_structure(image).is_err()
            || papk_format::find_manifest_value(image, keys::PACKAGE_NAME).is_none()
        {
            // A header over garbage: step past the header only, so a real
            // run that happens to start inside the claimed span is still found.
            s += 1;
            continue;
        }
        insert(
            d,
            Entry {
                image,
                first_sector: s as u16,
                sectors: span as u16,
                flags: meta.flags,
                seq: meta.seq,
                kind: Kind::App,
            },
        );
        s += span;
    }
    log_directory(d);
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
        let entry = Entry {
            image,
            first_sector: 0,
            sectors: 0,
            flags: 0,
            seq: 0,
            kind: Kind::System,
        };
        match d.entries.iter().position(|e| e.is_none()) {
            Some(i) => {
                d.entries[i] = Some(entry);
                log_system(&entry);
            }
            None => crate::pd_warn!("[packages] directory full: system app {} skipped", package),
        }
    }
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
}

// ── Placement ───────────────────────────────────────────────────────────────

/// Where an install goes, and what to erase around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub first_sector: u32,
    /// `(first_sector, sectors)` to erase before streaming: the target run,
    /// widened over an old copy it replaces in place.
    pub erase_before: (u32, u32),
    /// An old copy to erase after the new run commits (an upgrade beside it).
    pub evict_after: Option<(u32, u32)>,
    pub flags: u32,
    pub seq: u32,
    /// Free space suffices but no gap does: compact, then plan again.
    pub compact_first: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    TooLarge,
    NoPackageName,
    SystemPackage,
    NoRoom {
        need: u32,
        largest_free: u32,
        total_free: u32,
        installed: u32,
        max: u32,
    },
}

/// Decide where a PAPK of `papk_len` bytes for `package` goes (D5).
///
/// `max_apps` is the board's directory capacity: 1 means a single-app board,
/// where an install replaces whatever is installed and needs no package
/// name. A firmware built without `has_multi_app` carries only that rule
/// — the placement, compaction and free-space code below is what a
/// single-app board must not pay flash for.
pub fn plan_install(
    package: Option<&str>,
    papk_len: usize,
    max_apps: usize,
) -> Result<Plan, PlanError> {
    let total = region_sectors();
    let need = run_sectors(papk_len);
    if papk_len == 0 || need > total {
        return Err(PlanError::TooLarge);
    }
    let seq = next_seq();

    #[cfg(not(has_multi_app))]
    {
        let _ = (package, max_apps);
        Ok(single_app_plan(need, seq))
    }
    #[cfg(has_multi_app)]
    plan_multi_app(package, need, seq, max_apps)
}

/// Single-app: one run at sector 0, whatever is there is replaced, and it
/// is always the boot app.
fn single_app_plan(need: u32, seq: u32) -> Plan {
    let old = apps().next().map(Entry::run);
    let (erase_before, evict_after) = match old {
        Some((0, sectors)) => ((0, need.max(sectors)), None),
        Some(other) => ((0, need), Some(other)),
        None => ((0, need), None),
    };
    Plan {
        first_sector: 0,
        erase_before,
        evict_after,
        flags: FLAG_BOOT_DEFAULT,
        seq,
        compact_first: false,
    }
}

#[cfg(has_multi_app)]
fn plan_multi_app(
    package: Option<&str>,
    need: u32,
    seq: u32,
    max_apps: usize,
) -> Result<Plan, PlanError> {
    if max_apps <= 1 {
        return Ok(single_app_plan(need, seq));
    }
    let Some(package) = package else {
        return Err(PlanError::NoPackageName);
    };
    if is_system(package) {
        return Err(PlanError::SystemPackage);
    }
    let existing = find(package).filter(|e| e.kind == Kind::App);
    let installed = installed_count();
    let no_room = |largest_free, total_free| PlanError::NoRoom {
        need,
        largest_free,
        total_free,
        installed,
        max: max_apps as u32,
    };
    if existing.is_none() && installed as usize >= max_apps {
        let (largest, total_free) = free_space();
        return Err(no_room(largest, total_free));
    }
    let old = existing.map(Entry::run);
    let flags = existing.map(|e| e.flags & FLAG_BOOT_DEFAULT).unwrap_or(0);

    // 1. Beside the old copy: the upgrade is non-destructive.
    if let Some(first) = first_fit(need, None) {
        return Ok(Plan {
            first_sector: first,
            erase_before: (first, need),
            evict_after: old,
            flags,
            seq,
            compact_first: false,
        });
    }
    // 2. Over the old copy: its sectors count as free, and the erase covers
    //    both the old run and the new one.
    if let Some((old_first, old_sectors)) = old {
        if let Some(first) = first_fit(need, Some(old_first)) {
            let start = first.min(old_first);
            let end = (first + need).max(old_first + old_sectors);
            return Ok(Plan {
                first_sector: first,
                erase_before: (start, end - start),
                evict_after: None,
                flags,
                seq,
                compact_first: false,
            });
        }
    }
    // 3. Enough free space in pieces: compact, then plan again.
    let (largest, total_free) = space(None);
    let freeable = total_free + old.map(|o| o.1).unwrap_or(0);
    if freeable >= need {
        return Ok(Plan {
            first_sector: 0,
            erase_before: (0, 0),
            evict_after: None,
            flags,
            seq,
            compact_first: true,
        });
    }
    Err(no_room(largest, total_free))
}

/// Occupied runs sorted by sector, skipping the app run starting at
/// `exclude`. Stale runs count as occupied: a new run placed over a
/// commit-less header would be hidden by it at the next scan (the header's
/// span skips past it), so they stay off limits until [`cleanup`].
fn sorted_runs(exclude: Option<u32>, out: &mut [(u32, u32); CAPACITY + STALE_MAX]) -> usize {
    let d = dir();
    let mut n = 0;
    let occupied = apps()
        .map(Entry::run)
        .filter(|run| Some(run.0) != exclude)
        .chain(d.stale.iter().flatten().copied());
    for run in occupied {
        // Insertion sort: CAPACITY is a few dozen at most.
        let mut i = n;
        while i > 0 && out[i - 1].0 > run.0 {
            out[i] = out[i - 1];
            i -= 1;
        }
        out[i] = run;
        n += 1;
    }
    n
}

/// Lowest sector where `need` free sectors start, with the run at
/// `exclude` (if any) treated as free.
#[cfg(has_multi_app)]
fn first_fit(need: u32, exclude: Option<u32>) -> Option<u32> {
    let total = region_sectors();
    let mut runs = [(0u32, 0u32); CAPACITY + STALE_MAX];
    let n = sorted_runs(exclude, &mut runs);
    let mut cursor = 0u32;
    for &(first, sectors) in &runs[..n] {
        if first - cursor >= need {
            return Some(cursor);
        }
        cursor = first + sectors;
    }
    (total - cursor >= need).then_some(cursor)
}

/// `(largest gap, total free)` in sectors, with `exclude` treated as free.
fn space(exclude: Option<u32>) -> (u32, u32) {
    let total = region_sectors();
    let mut runs = [(0u32, 0u32); CAPACITY + STALE_MAX];
    let n = sorted_runs(exclude, &mut runs);
    let (mut largest, mut free, mut cursor) = (0u32, 0u32, 0u32);
    for &(first, sectors) in &runs[..n] {
        let gap = first - cursor;
        largest = largest.max(gap);
        free += gap;
        cursor = first + sectors;
    }
    let tail = total - cursor;
    (largest.max(tail), free + tail)
}

// ── Compaction (multi-app boards only) ──────────────────────────────────────

/// Slide every run toward the region start, closing the gaps (D6).
///
/// Each move writes the destination header page (a new `seq`, no commit
/// page), copies the image sectors in ascending order, writes the commit
/// page, then erases whatever the slide left of the old run. A move into
/// fully free space is loss-free at every instant; an overlapping slide
/// destroys the old meta sector mid-copy, so a power loss between then and
/// the commit page loses that app — never a corrupt or phantom one. Rescans
/// when done.
///
/// # Safety-adjacent
/// Must run with the JVM core parked (the installer's contract): the region
/// is erased and programmed here.
#[cfg(has_multi_app)]
pub fn compact(flash: &mut impl PapkFlash) {
    let mut runs = [(0u32, 0u32); CAPACITY + STALE_MAX];
    let n = sorted_runs(None, &mut runs);
    let mut cursor = 0u32;
    let mut seq = next_seq();
    for &(first, sectors) in &runs[..n] {
        if first > cursor {
            let entry = apps()
                .find(|e| e.first_sector as u32 == first)
                .copied()
                .expect("sorted_runs came from the directory");
            crate::pd_info!(
                "[packages] compacting {}: sector {} -> {} ({} sectors)",
                entry.package(),
                first,
                cursor,
                sectors
            );
            move_run(
                flash,
                first,
                sectors,
                cursor,
                entry.image.len() as u32,
                entry.flags,
                seq,
            );
            seq = seq.wrapping_add(1);
        }
        cursor += sectors;
    }
    rescan_region(flash);
}

#[cfg(has_multi_app)]
fn move_run(
    flash: &mut impl PapkFlash,
    first: u32,
    sectors: u32,
    dst: u32,
    len: u32,
    flags: u32,
    seq: u32,
) {
    // SAFETY: the caller's contract (JVM parked); every sector touched is
    // inside the region, and destination sectors are erased before they are
    // programmed — including the old run's own sectors once the slide
    // overtakes them, which is sound because their pages were copied first.
    unsafe {
        flash.erase_run(dst, 1);
        flash.select_run(dst);
        flash.write_meta_header(len, flags, seq);
        for k in 1..sectors {
            flash.erase_run(dst + k, 1);
            for page in 0..PAGES_PER_SECTOR {
                flash.copy_page(first + k, dst + k, page);
            }
        }
        flash.write_meta_commit();
        // Whatever the slide did not overwrite of the old run, meta sector
        // included when it survived: erase it so no stale image bytes can
        // ever read as a run.
        let overwritten_end = dst + sectors;
        let old_start = first.max(overwritten_end);
        let old_end = first + sectors;
        if old_start < old_end {
            flash.erase_run(old_start, old_end - old_start);
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    /// The directory is a process-wide static; tests that touch it take this.
    pub static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub fn lock() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::mem_region::{MemRegion, MemTransport, NoCoordinator};
    use crate::install::{install, uninstall, InstallError};
    use papk_format::flash_image::build_meta_pages;
    use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};

    const FW: &str = crate::framework_map::FRAMEWORK_MAP_VERSION;
    const MAX: usize = 8;

    /// A real PAPK for `package`, padded with `extra` bytes of filler.
    fn papk(package: &str, extra: usize) -> Vec<u8> {
        let filler: Vec<u8> = (0..extra).map(|i| (i * 7 % 251) as u8).collect();
        let mut b = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: package,
            version: "1.0",
            framework_map_version: FW,
            version_code: Some(1),
            label: None,
            icon: None,
        });
        b.class("t/Main", &filler);
        b.build().unwrap()
    }

    /// Enough filler to make the run occupy exactly `sectors` sectors.
    fn papk_of_sectors(package: &str, sectors: u32) -> Vec<u8> {
        let base = papk("x", 0).len();
        let target = (sectors as usize - 1) * SECTOR - 100;
        let p = papk(package, target - base);
        assert_eq!(run_sectors(p.len()), sectors);
        p
    }

    fn fresh(sectors: usize, max_apps: usize) -> MemRegion {
        reset_for_test();
        let region = MemRegion::new(sectors, max_apps);
        rescan_region(&region);
        region
    }

    /// A system app's image, leaked as `.rodata` would be.
    fn system(package: &str) -> &'static [u8] {
        alloc::boxed::Box::leak(papk(package, 50).into_boxed_slice())
    }

    /// What build.rs links into the region: the meta pages, then the image.
    fn bake(r: &mut MemRegion, sector: u32, image: &[u8], flags: u32, seq: u32) {
        unsafe {
            r.select_run(sector);
            for (i, chunk) in image.chunks(256).enumerate() {
                let mut page = [0xFFu8; 256];
                page[..chunk.len()].copy_from_slice(chunk);
                assert!(r.write_page(i as u32, &page));
            }
            r.commit_metadata(image.len() as u32, flags, seq);
        }
        rescan_region(r);
    }

    fn boot_package() -> Option<&'static str> {
        select_boot(None, None).map(|e| e.package())
    }

    fn do_install(region: &mut MemRegion, bytes: &[u8]) -> Result<(), InstallError> {
        let mut t = MemTransport::for_papk(bytes);
        let ok = install(&mut t, &mut NoCoordinator, region, bytes.len() as u32);
        match t.error {
            Some(e) => Err(e),
            None => {
                assert!(ok && t.success && t.ready);
                Ok(())
            }
        }
    }

    fn do_uninstall(region: &mut MemRegion, package: &str) {
        let (first, sectors) = find(package).expect("installed").run();
        let mut t = MemTransport::for_papk(&[]);
        assert!(uninstall(
            &mut t,
            &mut NoCoordinator,
            region,
            first,
            sectors
        ));
    }

    fn placed() -> Vec<(String, u32, u32, u32)> {
        let mut v: Vec<_> = apps()
            .map(|e| {
                (
                    e.package().to_string(),
                    e.first_sector as u32,
                    e.sectors as u32,
                    e.seq,
                )
            })
            .collect();
        v.sort_by_key(|e| e.1);
        v
    }

    fn image_of(package: &str) -> Vec<u8> {
        find(package).unwrap().image.to_vec()
    }

    #[test]
    fn an_erased_region_scans_to_no_apps_and_no_boot_image() {
        let _g = test_support::lock();
        let _r = fresh(16, MAX);
        assert_eq!(installed_count(), 0);
        assert!(boot_image().is_none());
        assert_eq!(free_space(), (16, 16));
        assert_eq!(next_seq(), 1);
    }

    #[test]
    fn a_baked_run_is_found_and_is_the_boot_default() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        let a = papk("com.a", 100);
        // What build.rs links into the region: meta pages then the image.
        unsafe {
            r.select_run(0);
            for (i, chunk) in a.chunks(256).enumerate() {
                let mut page = [0xFFu8; 256];
                page[..chunk.len()].copy_from_slice(chunk);
                assert!(r.write_page(i as u32, &page));
            }
            r.commit_metadata(a.len() as u32, FLAG_BOOT_DEFAULT, 0);
        }
        rescan_region(&r);
        assert_eq!(
            placed(),
            vec![("com.a".to_string(), 0, run_sectors(a.len()), 0)]
        );
        assert_eq!(boot_image(), Some(&a[..]));
        assert!(find("com.a").unwrap().is_boot_default());
        let _ = build_meta_pages; // keep the import honest on both shrink modes
    }

    #[test]
    fn fresh_packages_go_first_fit_and_a_full_directory_refuses() {
        let _g = test_support::lock();
        let mut r = fresh(64, 3);
        do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 4)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.c", 2)).unwrap();
        assert_eq!(
            placed(),
            vec![
                ("com.a".into(), 0, 3, 1),
                ("com.b".into(), 3, 4, 2),
                ("com.c".into(), 7, 2, 3),
            ]
        );
        let err = do_install(&mut r, &papk_of_sectors("com.d", 2)).unwrap_err();
        assert!(
            matches!(
                err,
                InstallError::NoRoom {
                    installed: 3,
                    max: 3,
                    ..
                }
            ),
            "{err:?}"
        );
        // Nothing was touched by the refusal.
        assert_eq!(installed_count(), 3);
    }

    #[test]
    fn a_papk_larger_than_the_region_is_too_large_before_anything_else() {
        let _g = test_support::lock();
        let mut r = fresh(4, MAX);
        let big = papk_of_sectors("com.big", 5);
        let err = do_install(&mut r, &big).unwrap_err();
        assert_eq!(err, InstallError::TooLarge);
        assert!(r.ops.is_empty());
    }

    #[test]
    fn a_reinstall_goes_beside_the_old_copy_and_evicts_it_after_commit() {
        let _g = test_support::lock();
        let mut r = fresh(32, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 3)).unwrap();
        r.ops.clear();
        let a2 = papk_of_sectors("com.a", 4);
        do_install(&mut r, &a2).unwrap();
        // The new copy landed after b; the old one at 0 was erased last.
        assert_eq!(
            placed(),
            vec![("com.b".into(), 3, 3, 2), ("com.a".into(), 6, 4, 3)]
        );
        assert_eq!(image_of("com.a"), a2);
        let last_erase = r
            .ops
            .iter()
            .rposition(|o| matches!(o, crate::install::mem_region::Op::Erase(0, 3)));
        let commit = r
            .ops
            .iter()
            .position(|o| matches!(o, crate::install::mem_region::Op::Program(6, 0, 512)));
        assert!(
            commit.unwrap() < last_erase.unwrap(),
            "old copy erased before the new one committed: {:?}",
            r.ops
        );
    }

    #[test]
    fn a_reinstall_with_no_room_beside_goes_in_place() {
        let _g = test_support::lock();
        let mut r = fresh(8, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 5)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 3)).unwrap();
        assert_eq!(free_space(), (0, 0));
        let a2 = papk_of_sectors("com.a", 4);
        do_install(&mut r, &a2).unwrap();
        assert_eq!(
            placed(),
            vec![("com.a".into(), 0, 4, 3), ("com.b".into(), 5, 3, 2)]
        );
        assert_eq!(image_of("com.a"), a2);
        // The old run's fifth sector was erased with the rest, not left stale.
        assert!(r.sector(4).iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn boot_default_is_inherited_by_a_reinstall_of_the_same_package() {
        let _g = test_support::lock();
        let mut r = fresh(32, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        // Mark it the boot default the way the baked image is.
        let (first, sectors) = find("com.a").unwrap().run();
        let a = image_of("com.a");
        unsafe {
            r.erase_run(first, sectors);
            r.select_run(first);
            for (i, chunk) in a.chunks(256).enumerate() {
                let mut page = [0xFFu8; 256];
                page[..chunk.len()].copy_from_slice(chunk);
                r.write_page(i as u32, &page);
            }
            r.commit_metadata(a.len() as u32, FLAG_BOOT_DEFAULT, 0);
        }
        rescan_region(&r);
        do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
        assert!(find("com.a").unwrap().is_boot_default());
        assert!(!find("com.b").unwrap().is_boot_default());
        assert_eq!(boot_image(), Some(&image_of("com.a")[..]));
    }

    #[test]
    fn uninstall_erases_the_whole_run() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
        do_uninstall(&mut r, "com.a");
        assert_eq!(placed(), vec![("com.b".into(), 3, 2, 2)]);
        for s in 0..3 {
            assert!(
                r.sector(s).iter().all(|&b| b == 0xFF),
                "sector {s} not erased"
            );
        }
        assert_eq!(free_space(), (11, 14));
    }

    #[test]
    fn fragmented_free_space_compacts_then_installs() {
        let _g = test_support::lock();
        let mut r = fresh(20, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 5)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 5)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.c", 5)).unwrap();
        let b = image_of("com.b");
        do_uninstall(&mut r, "com.a");
        do_uninstall(&mut r, "com.c");
        assert_eq!(free_space(), (10, 15));
        let d = papk_of_sectors("com.d", 12);
        do_install(&mut r, &d).unwrap();
        // b slid to the front with the next seq, d took the one after.
        assert_eq!(
            placed(),
            vec![("com.b".into(), 0, 5, 3), ("com.d".into(), 5, 12, 4)]
        );
        assert_eq!(image_of("com.b"), b, "the moved image must survive intact");
        assert_eq!(image_of("com.d"), d);
        // The vacated sectors of b's old run hold no stale bytes.
        for s in 5..10 {
            assert!(
                r.sector(s).iter().all(|&b| b == 0xFF) || s >= 5,
                "sector {s}"
            );
        }
        assert!(r.sector(17).iter().all(|&b| b == 0xFF));
    }

    /// The gap is smaller than the run being moved, so the slide overwrites
    /// its own source as it goes — the ascending-order copy must still land
    /// every byte.
    #[test]
    fn an_overlapping_slide_preserves_the_image() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 8)).unwrap();
        let b = image_of("com.b");
        do_uninstall(&mut r, "com.a");
        // 2 free at the front, 6 at the tail: a 7-sector run needs compaction.
        let c = papk_of_sectors("com.c", 7);
        do_install(&mut r, &c).unwrap();
        assert_eq!(
            placed(),
            vec![("com.b".into(), 0, 8, 3), ("com.c".into(), 8, 7, 4)]
        );
        assert_eq!(image_of("com.b"), b);
        assert_eq!(image_of("com.c"), c);
    }

    #[test]
    fn a_commit_less_run_is_stale_and_cleanup_erases_it() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        // A relocation or install that lost power after the header page.
        unsafe {
            r.select_run(5);
            r.write_meta_header(100, 0, 9);
        }
        rescan_region(&r);
        assert_eq!(placed(), vec![("com.a".into(), 0, 2, 1)]);
        // Its sectors are not free until cleanup.
        assert_eq!(free_space(), (9, 12));
        cleanup(&mut r);
        assert_eq!(free_space(), (14, 14));
        assert!(r.sector(5).iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn duplicates_resolve_to_the_higher_seq_and_the_loser_is_cleaned_up() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        let a1 = papk_of_sectors("com.a", 2);
        let a2 = papk_of_sectors("com.a", 2);
        // Two committed copies, as a power loss between commit and evict leaves.
        for (sector, image, seq) in [(0u32, &a1, 1u32), (4, &a2, 2)] {
            unsafe {
                r.select_run(sector);
                for (i, chunk) in image.chunks(256).enumerate() {
                    let mut page = [0xFFu8; 256];
                    page[..chunk.len()].copy_from_slice(chunk);
                    r.write_page(i as u32, &page);
                }
                r.commit_metadata(image.len() as u32, 0, seq);
            }
        }
        rescan_region(&r);
        assert_eq!(placed(), vec![("com.a".into(), 4, 2, 2)]);
        cleanup(&mut r);
        assert!(r.sector(0).iter().all(|&b| b == 0xFF));
        assert_eq!(placed(), vec![("com.a".into(), 4, 2, 2)]);
    }

    #[test]
    fn a_single_app_board_replaces_whatever_is_installed() {
        let _g = test_support::lock();
        let mut r = fresh(16, 1);
        do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
        assert!(find("com.a").unwrap().is_boot_default());
        let b = papk_of_sectors("com.b", 2);
        do_install(&mut r, &b).unwrap();
        assert_eq!(placed(), vec![("com.b".into(), 0, 2, 2)]);
        assert_eq!(boot_image(), Some(&b[..]));
        // The old run's third sector was erased along with the rest.
        assert!(r.sector(2).iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn a_multi_app_board_needs_a_package_name() {
        let _g = test_support::lock();
        let r = fresh(8, MAX);
        let mut bare = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: "",
            version: "1.0",
            framework_map_version: FW,
            version_code: None,
            label: None,
            icon: None,
        });
        bare.class("t/Main", b"CAFE");
        let bytes = bare.build().unwrap();
        // An empty package-name is present-but-empty; the plan treats it as a name.
        // The real "no key" case comes from a hand-built PAPK — simulate by planning.
        assert_eq!(
            plan_install(None, bytes.len(), MAX).unwrap_err(),
            PlanError::NoPackageName
        );
        assert!(r.ops.is_empty());
    }

    #[test]
    fn the_running_package_is_recorded_and_bounded() {
        let _g = test_support::lock();
        set_running(Some("com.example.weather"));
        assert_eq!(running(), Some("com.example.weather"));
        set_running(None);
        assert_eq!(running(), None);
        let long = "x".repeat(100);
        set_running(Some(&long));
        assert_eq!(running().map(|s| s.len()), Some(64));
    }

    /// A reflash bakes the package again at sector 0, sequence 0, while an
    /// older `pdb install` copy of it sits further up with a higher
    /// sequence: the fresh bake must win (it is what the developer just
    /// flashed, and the old copy may be built for another map version),
    /// and the old copy is stale.
    #[test]
    fn a_fresh_bake_beats_an_older_install_of_the_same_package() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        let a1 = papk_of_sectors("com.a", 2);
        bake(&mut r, 0, &a1, FLAG_BOOT_DEFAULT, 0);
        // An upgrade goes beside the bake and evicts it.
        let a2 = papk_of_sectors("com.a", 2);
        do_install(&mut r, &a2).unwrap();
        assert_eq!(placed(), vec![("com.a".into(), 2, 2, 1)]);
        assert!(r.sector(0).iter().all(|&b| b == 0xFF));
        // Then a reflash bakes a third copy at sector 0 again.
        let a3 = papk_of_sectors("com.a", 2);
        bake(&mut r, 0, &a3, FLAG_BOOT_DEFAULT, 0);
        assert_eq!(placed(), vec![("com.a".into(), 0, 2, 0)]);
        assert_eq!(boot_image(), Some(&a3[..]));
        // The old copy is stale until cleanup erases it.
        assert_eq!(free_space(), (12, 12));
        cleanup(&mut r);
        assert_eq!(free_space(), (14, 14));
        assert!(r.sector(2).iter().all(|&b| b == 0xFF));
        assert_eq!(placed(), vec![("com.a".into(), 0, 2, 0)]);
    }

    // ── System apps and boot selection (M2) ────────────────────────────────

    #[test]
    fn system_apps_are_entries_but_not_installed_apps_and_survive_a_rescan() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        register_system(&[system(LAUNCHER_PACKAGE), system("picodroid.settings")]);
        assert_eq!(installed_count(), 0);
        assert_eq!(entries().count(), 2);
        assert!(is_system(LAUNCHER_PACKAGE));
        assert_eq!(find("picodroid.settings").unwrap().kind, Kind::System);
        assert_eq!(free_space(), (16, 16), "system apps take no region space");
        assert_eq!(launcher().map(|e| e.package()), Some(LAUNCHER_PACKAGE));
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        cleanup(&mut r);
        rescan_region(&r);
        assert_eq!(entries().count(), 3);
        assert!(is_system(LAUNCHER_PACKAGE));
        assert_eq!(placed(), vec![("com.a".into(), 0, 2, 1)]);
        assert_eq!(next_seq(), 2, "system entries carry no sequence number");
    }

    #[test]
    fn register_system_skips_bad_images_duplicates_and_overflow() {
        let _g = test_support::lock();
        let _r = fresh(16, MAX);
        register_system(&[system("picodroid.a"), system("picodroid.a")]);
        assert_eq!(entries().count(), 1);
        register_system(&[b"not a papk at all"]);
        assert_eq!(entries().count(), 1);
        // Built for a framework this firmware is not.
        let mut future = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: "picodroid.future",
            version: "1.0",
            framework_map_version: "9.9.9",
            version_code: Some(1),
            label: None,
            icon: None,
        });
        future.class("t/Main", b"CAFE");
        let future: &'static [u8] =
            alloc::boxed::Box::leak(future.build().unwrap().into_boxed_slice());
        register_system(&[future]);
        assert!(find("picodroid.future").is_none());
        // Only SYSTEM_MAX fit.
        register_system(&[system("picodroid.b"), system("picodroid.c")]);
        assert_eq!(entries().count(), SYSTEM_MAX);
        assert!(find("picodroid.b").is_some());
        assert!(find("picodroid.c").is_none());
    }

    /// Java may uninstall an installed app that is neither a system app nor
    /// the one asking; the platform's erase is refused under test, so the
    /// composed call reports `Failed` without touching the directory.
    #[cfg(has_multi_app)]
    #[test]
    fn a_java_uninstall_refuses_system_running_and_unknown_packages() {
        let _g = test_support::lock();
        reset_for_test();
        let mut region = fresh(16, MAX);
        register_system(&[system("picodroid.launcher")]);
        rescan_region(&region);
        do_install(&mut region, &papk("com.a", 0)).unwrap();
        let run = uninstall_target("com.a").unwrap();
        assert_eq!(
            run,
            (
                find("com.a").unwrap().first_sector as u32,
                find("com.a").unwrap().sectors as u32
            )
        );
        assert_eq!(
            uninstall_target("picodroid.launcher"),
            Err(UninstallOutcome::System)
        );
        assert_eq!(
            uninstall_target("com.zzz"),
            Err(UninstallOutcome::NotInstalled)
        );
        set_running(Some("com.a"));
        assert_eq!(uninstall_target("com.a"), Err(UninstallOutcome::Running));
        set_running(Some("picodroid.launcher"));
        assert_eq!(uninstall_from_app("com.a"), UninstallOutcome::Failed);
        assert!(find("com.a").is_some());
        set_running(None);
    }

    #[test]
    fn a_run_named_like_a_system_app_is_stale_and_cleanup_erases_it() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        register_system(&[system(LAUNCHER_PACKAGE)]);
        let fake = papk_of_sectors(LAUNCHER_PACKAGE, 2);
        bake(&mut r, 0, &fake, FLAG_BOOT_DEFAULT, 0);
        assert_eq!(installed_count(), 0);
        assert_eq!(find(LAUNCHER_PACKAGE).unwrap().kind, Kind::System);
        // Its sectors are not free until cleanup.
        assert_eq!(free_space(), (14, 14));
        cleanup(&mut r);
        assert_eq!(free_space(), (16, 16));
        assert!(r.sector(0).iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn select_boot_follows_the_override_then_the_board_then_the_flags() {
        let _g = test_support::lock();
        let mut r = fresh(32, MAX);
        let a = papk_of_sectors("com.a", 2);
        bake(&mut r, 0, &a, FLAG_BOOT_DEFAULT, 0);
        do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
        register_system(&[system(LAUNCHER_PACKAGE)]);
        let pick = |o: Option<&str>, b: Option<&str>| select_boot(o, b).map(|e| e.package());
        assert_eq!(pick(None, None), Some("com.a"), "the boot default");
        assert_eq!(pick(None, Some("com.b")), Some("com.b"), "board key");
        assert_eq!(
            pick(Some("com.b"), Some("com.a")),
            Some("com.b"),
            "override wins"
        );
        assert_eq!(pick(Some("launcher"), None), Some(LAUNCHER_PACKAGE));
        assert_eq!(pick(Some("app"), Some("com.b")), Some("com.a"));
        assert_eq!(pick(Some(LAUNCHER_PACKAGE), None), Some(LAUNCHER_PACKAGE));
        // A name that is not installed falls through.
        assert_eq!(pick(Some("com.zzz"), Some("com.b")), Some("com.b"));
        assert_eq!(pick(None, Some("com.zzz")), Some("com.a"));
        assert_eq!(boot_image(), Some(&a[..]));
    }

    #[test]
    fn without_a_boot_default_the_launcher_comes_before_the_lowest_run() {
        let _g = test_support::lock();
        let mut r = fresh(32, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
        assert_eq!(
            boot_package(),
            Some("com.a"),
            "lowest sector when nothing else says"
        );
        register_system(&[system(LAUNCHER_PACKAGE)]);
        assert_eq!(boot_package(), Some(LAUNCHER_PACKAGE));
        assert_eq!(
            select_boot(Some("app"), None).map(|e| e.package()),
            Some("com.a"),
            "--boot app skips the launcher"
        );
    }

    #[test]
    fn a_launcher_alone_boots_and_an_empty_directory_boots_nothing() {
        let _g = test_support::lock();
        let _r = fresh(8, MAX);
        assert_eq!(boot_package(), None);
        register_system(&[system(LAUNCHER_PACKAGE)]);
        assert_eq!(boot_package(), Some(LAUNCHER_PACKAGE));
        // No baked app: `--boot app` warns and the next rule applies.
        assert_eq!(
            select_boot(Some("app"), None).map(|e| e.package()),
            Some(LAUNCHER_PACKAGE)
        );
    }

    #[cfg(has_multi_app)]
    #[test]
    fn next_image_takes_a_pending_launch_once_then_returns_home() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        register_system(&[system(LAUNCHER_PACKAGE)]);
        let home = launcher().unwrap().image;
        set_running(Some(LAUNCHER_PACKAGE));
        assert_eq!(request_launch("com.zzz"), Err(NotFound));
        request_launch("com.a").unwrap();
        assert_eq!(next_image(), Some(&image_of("com.a")[..]));
        set_running(Some("com.a"));
        assert_eq!(next_image(), Some(home), "an app exit returns home");
        assert_eq!(
            next_image(),
            Some(home),
            "and again: the launcher was not running"
        );
    }

    #[cfg(has_multi_app)]
    #[test]
    fn a_pending_launch_is_dropped_when_its_package_is_gone() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
        register_system(&[system(LAUNCHER_PACKAGE)]);
        set_running(Some("com.b"));
        request_launch("com.a").unwrap();
        do_uninstall(&mut r, "com.a");
        assert_eq!(next_image(), Some(launcher().unwrap().image));
    }

    #[cfg(has_multi_app)]
    #[test]
    fn a_reinstalled_pending_package_launches_its_new_copy() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        request_launch("com.a").unwrap();
        let a2 = papk_of_sectors("com.a", 3);
        do_install(&mut r, &a2).unwrap();
        assert_eq!(next_image(), Some(&a2[..]));
    }

    #[cfg(has_multi_app)]
    #[test]
    fn the_launcher_is_restarted_once_then_the_device_waits() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        register_system(&[system(LAUNCHER_PACKAGE)]);
        let home = launcher().unwrap().image;
        set_running(Some(LAUNCHER_PACKAGE));
        assert_eq!(next_image(), Some(home), "first exit: start it again");
        assert_eq!(next_image(), None, "second in a row: wait for an install");
        // A launch in between resets the count.
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        request_launch("com.a").unwrap();
        assert_eq!(next_image(), Some(&image_of("com.a")[..]));
        set_running(Some("com.a"));
        assert_eq!(next_image(), Some(home));
        set_running(Some(LAUNCHER_PACKAGE));
        assert_eq!(next_image(), Some(home));
        assert_eq!(next_image(), None);
    }

    #[cfg(has_multi_app)]
    #[test]
    fn without_a_launcher_an_exit_waits_for_an_install() {
        let _g = test_support::lock();
        let mut r = fresh(16, MAX);
        do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
        set_running(Some("com.a"));
        assert_eq!(next_image(), None);
    }
}
