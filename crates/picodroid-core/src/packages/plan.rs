// SPDX-License-Identifier: GPL-3.0-only
//! Where an install goes (D5) and, on multi-app boards, compaction: sliding
//! the runs down over the gaps below them so the free space is one piece.
//!
//! Reads the directory through the parent module's accessors; the installer
//! reaches both through `install::CoreDirectory`.

use super::*;

// ── Placement ───────────────────────────────────────────────────────────────

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
pub(super) fn single_app_plan(need: u32, seq: u32) -> Plan {
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
pub(super) fn plan_multi_app(
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
pub(super) fn sorted_runs(
    exclude: Option<u32>,
    out: &mut [(u32, u32); CAPACITY + STALE_MAX],
) -> usize {
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
pub(super) fn first_fit(need: u32, exclude: Option<u32>) -> Option<u32> {
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
pub(super) fn space(exclude: Option<u32>) -> (u32, u32) {
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
pub(super) fn move_run(
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
