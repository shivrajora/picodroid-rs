// SPDX-License-Identifier: GPL-3.0-only
//! The PAPK flash slot, layered on a family's two flash primitives.
//!
//! [`PapkFlash`] is what the installer needs — erase a region, program a
//! page, commit the boot-meta sector, reset. Every family that keeps its app
//! in a fixed NOR-flash slot answers those the same way: sector round-up
//! plus the meta sector on erase, bounds and offset arithmetic on write, a
//! [`papk_format::flash_image`] page programmed last on commit. That layer
//! was the RP family's; it is the same for any family, so it lives here and
//! a family supplies only what differs — where the slot is, how big, how to
//! erase and program a range, how to reset (`docs/designs/porting-seam-2026-09.md`
//! E5).
//!
//! [`read_mapped`] is the boot-time counterpart for a family whose flash is
//! memory-mapped: hand it the slot's mapped address and it returns the
//! installed image as a `'static` slice the class loader reads in place.

use core::marker::PhantomData;

use papk_format::flash_image::{build_meta_page, parse_meta, HEADER_LEN, META_SIZE};

use super::PapkFlash;

/// The raw NOR-flash primitives under a family's PAPK slot.
///
/// Offsets are flash-relative (0 = the start of flash), which is what a ROM
/// erase/program routine takes; the memory-mapped address, if any, is the
/// family's business ([`read_mapped`]).
///
/// # Safety
///
/// `erase_range` and `program_range` may be called only while the JVM core
/// is parked — the `CoreCoordinator` contract — because on a family that
/// executes in place from this flash, anything else faults. [`PapkSlot`]
/// inherits `run_install`'s park; an implementor that reaches these from
/// anywhere else does not.
pub unsafe trait PapkSlotFlash {
    /// Flash-relative offset of the boot-meta sector. The image follows it.
    const META_OFFSET: u32;
    /// Largest PAPK the slot holds: the slot size minus the meta sector.
    const MAX_DATA_SIZE: usize;
    /// Erase granularity, in bytes.
    const SECTOR_SIZE: usize;

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

    /// Reboot into whatever the slot now holds. Never returns.
    fn reset() -> !;
}

/// [`PapkFlash`] for any [`PapkSlotFlash`].
///
/// Zero-sized: the slot is a fixed region named by constants, not something
/// with instance state. `PapkSlot::<F>::new()` is what a family hands to
/// `run_pdb_task`.
pub struct PapkSlot<F: PapkSlotFlash>(PhantomData<F>);

impl<F: PapkSlotFlash> PapkSlot<F> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }

    /// Flash-relative offset of the first image byte.
    pub const DATA_OFFSET: u32 = F::META_OFFSET + META_SIZE as u32;
}

impl<F: PapkSlotFlash> Default for PapkSlot<F> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: every method delegates to `F`'s primitives under the same contract
// the trait states — `run_install` parks the JVM core before reaching any of
// them — and the arithmetic below keeps every program inside the slot.
unsafe impl<F: PapkSlotFlash> PapkFlash for PapkSlot<F> {
    fn max_data_size(&self) -> usize {
        F::MAX_DATA_SIZE
    }

    /// The meta sector plus enough whole sectors for the image.
    unsafe fn erase_region(&mut self, papk_len: usize) {
        let data = papk_len.div_ceil(F::SECTOR_SIZE) * F::SECTOR_SIZE;
        F::erase_range(F::META_OFFSET, META_SIZE + data);
    }

    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool {
        let offset_within_slot = page_index as usize * 256;
        if offset_within_slot + 256 > F::MAX_DATA_SIZE {
            return false;
        }
        F::program_range(Self::DATA_OFFSET + offset_within_slot as u32, page);
        true
    }

    /// Built before the primitive is called, so a family whose
    /// `program_range` drops XIP runs nothing but the ROM call with it off.
    unsafe fn commit_metadata(&mut self, len: u32) {
        let page = build_meta_page(len);
        F::program_range(F::META_OFFSET, &page);
    }

    fn trigger_reset(&mut self) -> ! {
        F::reset()
    }
}

/// The installed PAPK, read in place through a memory-mapped slot.
///
/// `slot_base` is the mapped address of the boot-meta sector; the image
/// follows it. Erased flash fails the magic check, which is how a
/// never-installed device takes the `None` path. Families without a mapped
/// flash copy the image out instead.
///
/// # Safety
///
/// `slot_base` must map at least `META_SIZE + max_data_size` readable bytes
/// for the life of the program, and no erase or program of the slot may be
/// in flight — which is why a device calls this once, before the scheduler
/// starts.
pub unsafe fn read_mapped(slot_base: *const u8, max_data_size: usize) -> Option<&'static [u8]> {
    let header = core::slice::from_raw_parts(slot_base, HEADER_LEN);
    let meta = parse_meta(header, max_data_size)?;
    let data = slot_base.add(META_SIZE);
    Some(core::slice::from_raw_parts(data, meta.len as usize))
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

    struct Mock;
    const META: u32 = 0x0010_0000;
    const SECTOR: usize = 4096;
    const MAX: usize = 4 * SECTOR;

    unsafe impl PapkSlotFlash for Mock {
        const META_OFFSET: u32 = META;
        const MAX_DATA_SIZE: usize = MAX;
        const SECTOR_SIZE: usize = SECTOR;
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

    fn run(f: impl FnOnce(&mut PapkSlot<Mock>)) -> Vec<Op> {
        let _serial = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        LOG.lock().unwrap().clear();
        let mut slot = PapkSlot::<Mock>::new();
        f(&mut slot);
        std::mem::take(&mut *LOG.lock().unwrap())
    }

    #[test]
    fn erase_covers_the_meta_sector_and_whole_data_sectors() {
        // 5000 bytes need two 4 KB sectors; plus the meta sector in front.
        let ops = run(|s| unsafe { s.erase_region(5000) });
        assert_eq!(ops, [Op::Erase(META, META_SIZE + 2 * SECTOR)]);
        // An exact multiple does not round up an extra sector.
        let ops = run(|s| unsafe { s.erase_region(2 * SECTOR) });
        assert_eq!(ops, [Op::Erase(META, META_SIZE + 2 * SECTOR)]);
    }

    #[test]
    fn pages_land_after_the_meta_sector_at_256_byte_steps() {
        let page = [0xA5u8; 256];
        let ops = run(|s| unsafe {
            assert!(s.write_page(0, &page));
            assert!(s.write_page(3, &page));
        });
        let data_offset = META + META_SIZE as u32;
        assert_eq!(
            ops,
            [
                Op::Program(data_offset, page.to_vec()),
                Op::Program(data_offset + 3 * 256, page.to_vec()),
            ]
        );
    }

    #[test]
    fn the_last_page_fits_and_the_one_after_is_refused_untouched() {
        let page = [1u8; 256];
        let last = (MAX / 256 - 1) as u32;
        let ops = run(|s| unsafe {
            assert!(s.write_page(last, &page));
            assert!(!s.write_page(last + 1, &page));
        });
        assert_eq!(ops.len(), 1, "a refused page must not reach flash");
    }

    #[test]
    fn commit_programs_exactly_the_boot_meta_page_at_the_meta_sector() {
        let ops = run(|s| unsafe { s.commit_metadata(4321) });
        assert_eq!(ops, [Op::Program(META, build_meta_page(4321).to_vec())]);
    }

    #[test]
    fn max_data_size_is_the_familys_constant() {
        assert_eq!(PapkSlot::<Mock>::new().max_data_size(), MAX);
        assert_eq!(PapkSlot::<Mock>::DATA_OFFSET, META + META_SIZE as u32);
    }

    #[test]
    fn a_mapped_slot_reads_back_its_image() {
        let payload: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        let mut image = vec![0xFFu8; META_SIZE + MAX];
        image[..256].copy_from_slice(&build_meta_page(payload.len() as u32));
        image[META_SIZE..META_SIZE + payload.len()].copy_from_slice(&payload);
        let got = unsafe { read_mapped(image.as_ptr(), MAX) }.expect("installed");
        assert_eq!(got, &payload[..]);
    }

    #[test]
    fn an_erased_slot_reads_as_no_install() {
        let image = vec![0xFFu8; META_SIZE + MAX];
        assert!(unsafe { read_mapped(image.as_ptr(), MAX) }.is_none());
    }

    #[test]
    fn a_length_past_the_slot_reads_as_no_install() {
        let mut image = vec![0xFFu8; META_SIZE + MAX];
        image[..256].copy_from_slice(&build_meta_page(MAX as u32 + 1));
        assert!(unsafe { read_mapped(image.as_ptr(), MAX) }.is_none());
    }
}
