// SPDX-License-Identifier: GPL-3.0-only
//! The PAPK app region, layered on a family's two flash primitives.
//!
//! [`PapkFlash`] is what the installer and the package directory need —
//! erase a run, program a page, write the boot-meta pages, copy a page for a
//! relocation, reset. Every family that keeps its apps in NOR flash answers
//! those the same way: sector arithmetic on top of erase and program, and
//! [`papk_format::flash_image`] pages written in the order that keeps a run
//! either whole or visibly unfinished. That layer was the RP family's; it is
//! the same for any family, so it lives here and a family supplies only what
//! differs — where the region is, how big, how to erase and program a range,
//! where it is mapped, how to reset (`docs/designs/multi-app-2026-09.md` D3).
//!
//! A *run* is `[meta sector][PAPK padded to whole sectors]` at any sector of
//! the region; `first_sector` names it. The installer selects a run with
//! [`PapkFlash::select_run`] and then writes pages relative to it.

use core::marker::PhantomData;

use papk_format::flash_image::{
    build_commit_page, build_header_page, build_meta_pages, COMMIT_OFFSET, META_SIZE, PAGE_LEN,
};

use super::PapkFlash;

/// Flash program pages per sector.
pub const PAGES_PER_SECTOR: u32 = (META_SIZE / PAGE_LEN) as u32;

/// The raw NOR-flash primitives under a family's app region.
///
/// Offsets are flash-relative (0 = the start of flash), which is what a ROM
/// erase/program routine takes; the mapped address is what the directory
/// reads runs through.
///
/// # Safety
///
/// `erase_range` and `program_range` may be called only while the JVM core
/// is parked — the `CoreCoordinator` contract — because on a family that
/// executes in place from this flash, anything else faults. [`PapkRegion`]
/// inherits `run_install`'s park; an implementor that reaches these from
/// anywhere else does not. `mapped_base` must stay readable for the life of
/// the program and never alias RAM the caller writes.
pub unsafe trait PapkRegionFlash {
    /// Flash-relative offset of the region's first sector.
    const REGION_OFFSET: u32;
    /// Region length in bytes, a multiple of [`META_SIZE`].
    const REGION_LEN: usize;
    /// Erase granularity, in bytes; runs are aligned to it.
    const SECTOR_SIZE: usize;
    /// Package-directory capacity; 1 on a single-app board.
    const MAX_INSTALLED_APPS: usize;

    /// Mapped address of the region's first byte.
    fn mapped_base() -> *const u8;

    /// Erase `len` bytes at `flash_offset`; both are sector multiples.
    ///
    /// # Safety
    /// The JVM core must be parked. See the trait docs.
    unsafe fn erase_range(flash_offset: u32, len: usize);

    /// Program `data` at `flash_offset`; both are multiples of 256.
    ///
    /// # Safety
    /// The JVM core must be parked. See the trait docs.
    unsafe fn program_range(flash_offset: u32, data: &[u8]);

    /// Reboot into whatever the region now holds. Never returns.
    fn reset() -> !;
}

/// [`PapkFlash`] for any [`PapkRegionFlash`].
///
/// The one piece of state is the selected run; `PapkRegion::<F>::new()` is
/// what a family hands to `run_pdb_task`.
pub struct PapkRegion<F: PapkRegionFlash> {
    target: u32,
    _flash: PhantomData<F>,
}

impl<F: PapkRegionFlash> PapkRegion<F> {
    pub const fn new() -> Self {
        Self {
            target: 0,
            _flash: PhantomData,
        }
    }

    /// Flash-relative offset of a sector of the region.
    const fn sector_offset(sector: u32) -> u32 {
        F::REGION_OFFSET + sector * META_SIZE as u32
    }

    /// Flash-relative offset of the selected run's first image byte.
    fn data_offset(&self) -> u32 {
        Self::sector_offset(self.target) + META_SIZE as u32
    }
}

impl<F: PapkRegionFlash> Default for PapkRegion<F> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: every method delegates to `F`'s primitives under the same contract
// the trait states — `run_install` parks the JVM core before reaching any of
// them — and the arithmetic below keeps every program inside the region.
unsafe impl<F: PapkRegionFlash> PapkFlash for PapkRegion<F> {
    fn region_len(&self) -> usize {
        F::REGION_LEN
    }

    fn max_installed_apps(&self) -> usize {
        F::MAX_INSTALLED_APPS
    }

    fn mapped_base(&self) -> *const u8 {
        F::mapped_base()
    }

    fn select_run(&mut self, first_sector: u32) {
        self.target = first_sector;
    }

    unsafe fn erase_run(&mut self, first_sector: u32, sectors: u32) {
        F::erase_range(
            Self::sector_offset(first_sector),
            sectors as usize * META_SIZE,
        )
    }

    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool {
        let offset_in_run = META_SIZE + page_index as usize * PAGE_LEN;
        let run_start = self.target as usize * META_SIZE;
        if run_start + offset_in_run + PAGE_LEN > F::REGION_LEN {
            return false;
        }
        F::program_range(self.data_offset() + page_index * PAGE_LEN as u32, page);
        true
    }

    /// The pages are built before the primitive is called, so a family whose
    /// `program_range` drops XIP runs nothing but the ROM call with it off.
    unsafe fn write_meta_header(&mut self, len: u32, flags: u32, seq: u32) {
        let page = build_header_page(len, flags, seq);
        F::program_range(Self::sector_offset(self.target), &page);
    }

    unsafe fn write_meta_commit(&mut self) {
        let page = build_commit_page();
        F::program_range(
            Self::sector_offset(self.target) + COMMIT_OFFSET as u32,
            &page,
        );
    }

    unsafe fn commit_metadata(&mut self, len: u32, flags: u32, seq: u32) {
        let pages = build_meta_pages(len, flags, seq);
        F::program_range(Self::sector_offset(self.target), &pages);
    }

    /// Through a RAM copy: the ROM program routine reads its source while
    /// XIP is off, so it cannot take a pointer into the region itself.
    unsafe fn copy_page(&mut self, src_sector: u32, dst_sector: u32, page: u32) {
        let mut buf = [0u8; PAGE_LEN];
        let src = F::mapped_base().add(src_sector as usize * META_SIZE + page as usize * PAGE_LEN);
        core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), PAGE_LEN);
        F::program_range(
            Self::sector_offset(dst_sector) + page * PAGE_LEN as u32,
            &buf,
        );
    }

    fn trigger_reset(&mut self) -> ! {
        F::reset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, PartialEq, Eq)]
    enum Op {
        Erase(u32, usize),
        Program(u32, Vec<u8>),
    }

    // Associated functions have no `self` to record into, so the mock logs
    // through a static; tests serialise on `LOCK` and drain it.
    static LOG: Mutex<Vec<Op>> = Mutex::new(Vec::new());
    static LOCK: Mutex<()> = Mutex::new(());
    // A fake mapped region for copy_page reads: sector 2 holds a pattern.
    static MAPPED: [u8; 4 * SECTOR] = {
        let mut m = [0xFFu8; 4 * SECTOR];
        let mut i = 0;
        while i < SECTOR {
            m[2 * SECTOR + i] = (i % 251) as u8;
            i += 1;
        }
        m
    };

    struct Mock;
    const REGION: u32 = 0x0010_0000;
    const SECTOR: usize = 4096;
    const LEN: usize = 4 * SECTOR;

    unsafe impl PapkRegionFlash for Mock {
        const REGION_OFFSET: u32 = REGION;
        const REGION_LEN: usize = LEN;
        const SECTOR_SIZE: usize = SECTOR;
        const MAX_INSTALLED_APPS: usize = 2;
        fn mapped_base() -> *const u8 {
            MAPPED.as_ptr()
        }
        unsafe fn erase_range(flash_offset: u32, len: usize) {
            LOG.lock().unwrap().push(Op::Erase(flash_offset, len));
        }
        unsafe fn program_range(flash_offset: u32, data: &[u8]) {
            LOG.lock()
                .unwrap()
                .push(Op::Program(flash_offset, data.to_vec()));
        }
        fn reset() -> ! {
            panic!("__reset__")
        }
    }

    fn run(f: impl FnOnce(&mut PapkRegion<Mock>)) -> Vec<Op> {
        let _serial = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        LOG.lock().unwrap().clear();
        let mut region = PapkRegion::<Mock>::new();
        f(&mut region);
        std::mem::take(&mut *LOG.lock().unwrap())
    }

    #[test]
    fn a_run_is_erased_as_whole_sectors_at_its_sector() {
        let ops = run(|r| unsafe { r.erase_run(1, 2) });
        assert_eq!(ops, [Op::Erase(REGION + SECTOR as u32, 2 * SECTOR)]);
    }

    #[test]
    fn pages_land_after_the_selected_runs_meta_sector_at_256_byte_steps() {
        let page = [0xA5u8; 256];
        let ops = run(|r| unsafe {
            r.select_run(1);
            assert!(r.write_page(0, &page));
            assert!(r.write_page(3, &page));
        });
        let data_offset = REGION + SECTOR as u32 + META_SIZE as u32;
        assert_eq!(
            ops,
            [
                Op::Program(data_offset, page.to_vec()),
                Op::Program(data_offset + 3 * 256, page.to_vec()),
            ]
        );
    }

    #[test]
    fn the_last_page_of_the_region_fits_and_the_one_after_is_refused_untouched() {
        let page = [1u8; 256];
        // Run at sector 1: meta sector 1, data sectors 2..4 → 2 * 16 pages.
        let last = 2 * PAGES_PER_SECTOR - 1;
        let ops = run(|r| unsafe {
            r.select_run(1);
            assert!(r.write_page(last, &page));
            assert!(!r.write_page(last + 1, &page));
        });
        assert_eq!(ops.len(), 1, "a refused page must not reach flash");
    }

    #[test]
    fn commit_programs_both_meta_pages_at_the_selected_run() {
        let ops = run(|r| unsafe {
            r.select_run(3);
            r.commit_metadata(4321, 1, 7)
        });
        assert_eq!(
            ops,
            [Op::Program(
                REGION + 3 * SECTOR as u32,
                build_meta_pages(4321, 1, 7).to_vec()
            )]
        );
    }

    /// A relocation writes the header first, the commit page last, and each
    /// as its own program — never the same page twice.
    #[test]
    fn header_and_commit_are_separate_single_programs() {
        let ops = run(|r| unsafe {
            r.select_run(0);
            r.write_meta_header(64, 0, 9);
            r.write_meta_commit();
        });
        assert_eq!(
            ops,
            [
                Op::Program(REGION, build_header_page(64, 0, 9).to_vec()),
                Op::Program(REGION + COMMIT_OFFSET as u32, build_commit_page().to_vec()),
            ]
        );
    }

    #[test]
    fn copy_page_reads_the_mapped_source_and_programs_the_destination() {
        let ops = run(|r| unsafe { r.copy_page(2, 0, 1) });
        let expected: Vec<u8> = (256..512).map(|i| (i % 251) as u8).collect();
        assert_eq!(ops, [Op::Program(REGION + 256, expected)]);
    }

    #[test]
    fn the_region_constants_are_the_familys() {
        let r = PapkRegion::<Mock>::new();
        assert_eq!(r.region_len(), LEN);
        assert_eq!(r.max_data_size(), LEN - META_SIZE);
        assert_eq!(r.max_installed_apps(), 2);
        assert_eq!(r.mapped_base(), MAPPED.as_ptr());
    }
}
