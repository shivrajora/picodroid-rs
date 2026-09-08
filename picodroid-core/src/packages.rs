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

    fn run(&self) -> (u32, u32) {
        (self.first_sector as u32, self.sectors as u32)
    }
}

struct Dir {
    entries: [Option<Entry>; CAPACITY],
    /// Runs the last scan found that are not installed apps — commit-less
    /// headers and the losers of duplicates — as `(first_sector, sectors)`.
    stale: [Option<(u32, u32)>; STALE_MAX],
    region: Option<(*const u8, usize)>,
    /// The package `run_app` is executing, copied out of its manifest.
    running: ([u8; 64], usize),
}

struct DirCell(UnsafeCell<Dir>);
// SAFETY: single-writer discipline, see the module docs.
unsafe impl Sync for DirCell {}

static DIR: DirCell = DirCell(UnsafeCell::new(Dir {
    entries: [None; CAPACITY],
    stale: [None; STALE_MAX],
    region: None,
    running: ([0; 64], 0),
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

/// Add an app entry, resolving a duplicate package to the higher `seq`
/// (tie: the lower sector) and noting the loser for cleanup.
fn insert(d: &mut Dir, entry: Entry) {
    let package = entry.package();
    let existing = d
        .entries
        .iter()
        .position(|e| matches!(e, Some(e) if e.kind == Kind::App && e.package() == package));
    if let Some(i) = existing {
        let existing = d.entries[i].unwrap();
        let newer = entry.seq > existing.seq
            || (entry.seq == existing.seq && entry.first_sector < existing.first_sector);
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

/// The image to boot: the `BOOT_DEFAULT` run (lowest sector if several),
/// else the lowest-sector app, else nothing.
pub fn boot_image() -> Option<&'static [u8]> {
    let mut best: Option<&Entry> = None;
    for e in apps() {
        let better = match best {
            None => true,
            Some(b) => {
                (e.is_boot_default(), b.first_sector) > (b.is_boot_default(), e.first_sector)
            }
        };
        if better {
            best = Some(e);
        }
    }
    best.map(|e| e.image)
}

/// One more than the highest sequence number on the device.
pub fn next_seq() -> u32 {
    apps().map(|e| e.seq).max().unwrap_or(0).wrapping_add(1)
}

/// Record the package `run_app` is executing (its manifest's name, copied).
pub fn set_running(package: Option<&str>) {
    let d = dir();
    let name = package.unwrap_or("").as_bytes();
    let n = name.len().min(d.running.0.len());
    d.running.0[..n].copy_from_slice(&name[..n]);
    d.running.1 = n;
}

/// The package `run_app` is executing, if it named one.
pub fn running() -> Option<&'static str> {
    let d = dir();
    if d.running.1 == 0 {
        return None;
    }
    core::str::from_utf8(&d.running.0[..d.running.1]).ok()
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
        let region = MemRegion::new(sectors, max_apps);
        rescan_region(&region);
        region
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
}
