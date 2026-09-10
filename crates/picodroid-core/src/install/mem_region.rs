// SPDX-License-Identifier: GPL-3.0-only
//! An app region in memory: the [`PapkFlash`] the package-directory tests
//! and the simulator drive, with NOR semantics — erase sets a sector to
//! `0xFF`, and a program may only clear bits. Programming a byte that is not
//! erased panics, which is exactly the "no page programmed twice" rule the
//! boot-meta format relies on (`papk_format::flash_image`), caught the moment
//! any caller breaks it.

use alloc::vec::Vec;

use papk_format::flash_image::{
    build_commit_page, build_header_page, build_meta_pages, COMMIT_OFFSET, META_SIZE, PAGE_LEN,
};

use super::transport::{InstallError, InstallTransport, ReadError};
use super::{CoreCoordinator, PapkFlash};

/// The region, leaked so directory entries can point into it for the life
/// of the process, as they point into XIP flash on a device.
pub struct MemRegion {
    buf: &'static mut [u8],
    target: u32,
    max_apps: usize,
    /// Every (first_sector, sectors) erased and every page programmed, for
    /// tests that pin ordering.
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Erase(u32, u32),
    /// Program at a sector and byte offset within it, this many bytes.
    Program(u32, usize, usize),
}

impl MemRegion {
    /// A region of `sectors` erased sectors with room for `max_apps`.
    pub fn new(sectors: usize, max_apps: usize) -> Self {
        let buf = alloc::vec![0xFFu8; sectors * META_SIZE].into_boxed_slice();
        Self {
            buf: alloc::boxed::Box::leak(buf),
            target: 0,
            max_apps,
            ops: Vec::new(),
        }
    }

    pub fn sectors(&self) -> usize {
        self.buf.len() / META_SIZE
    }

    /// The bytes of a sector, as the directory reads them.
    pub fn sector(&self, sector: u32) -> &[u8] {
        let start = sector as usize * META_SIZE;
        &self.buf[start..start + META_SIZE]
    }

    fn program(&mut self, sector: u32, offset: usize, data: &[u8]) {
        let start = sector as usize * META_SIZE + offset;
        assert!(
            start + data.len() <= self.buf.len(),
            "program past the region: sector {sector} + {offset} + {}",
            data.len()
        );
        assert!(
            offset.is_multiple_of(PAGE_LEN) && data.len().is_multiple_of(PAGE_LEN),
            "program must be page-aligned: offset {offset}, len {}",
            data.len()
        );
        for (i, b) in data.iter().enumerate() {
            let cell = &mut self.buf[start + i];
            assert_eq!(
                *cell,
                0xFF,
                "sector {sector} byte {} programmed twice without an erase",
                offset + i
            );
            *cell = *b;
        }
        self.ops.push(Op::Program(sector, offset, data.len()));
    }
}

// SAFETY: the buffer is leaked, so `mapped_base` stays valid for the process;
// every write is bounds-checked above.
unsafe impl PapkFlash for MemRegion {
    fn region_len(&self) -> usize {
        self.buf.len()
    }

    fn max_installed_apps(&self) -> usize {
        self.max_apps
    }

    fn mapped_base(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    fn select_run(&mut self, first_sector: u32) {
        self.target = first_sector;
    }

    unsafe fn erase_run(&mut self, first_sector: u32, sectors: u32) {
        let start = first_sector as usize * META_SIZE;
        let end = start + sectors as usize * META_SIZE;
        assert!(end <= self.buf.len(), "erase past the region");
        self.buf[start..end].fill(0xFF);
        self.ops.push(Op::Erase(first_sector, sectors));
    }

    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool {
        let run_start = self.target as usize * META_SIZE;
        let offset = META_SIZE + page_index as usize * PAGE_LEN;
        if run_start + offset + PAGE_LEN > self.buf.len() {
            return false;
        }
        self.program(self.target, offset, page);
        true
    }

    unsafe fn write_meta_header(&mut self, len: u32, flags: u32, seq: u32) {
        let page = build_header_page(len, flags, seq);
        self.program(self.target, 0, &page);
    }

    unsafe fn write_meta_commit(&mut self) {
        let page = build_commit_page();
        self.program(self.target, COMMIT_OFFSET, &page);
    }

    unsafe fn commit_metadata(&mut self, len: u32, flags: u32, seq: u32) {
        let pages = build_meta_pages(len, flags, seq);
        self.program(self.target, 0, &pages);
    }

    unsafe fn copy_page(&mut self, src_sector: u32, dst_sector: u32, page: u32) {
        let mut buf = [0u8; PAGE_LEN];
        let src = src_sector as usize * META_SIZE + page as usize * PAGE_LEN;
        buf.copy_from_slice(&self.buf[src..src + PAGE_LEN]);
        self.program(dst_sector, page as usize * PAGE_LEN, &buf);
    }

    fn trigger_reset(&mut self) -> ! {
        panic!("MemRegion has no reset; drive `install`/`uninstall`, not `run_*`")
    }
}

/// A transport that replays a PAPK plus its CRC, as the wire would carry it,
/// and records what the installer reported.
pub struct MemTransport {
    data: Vec<u8>,
    pos: usize,
    pub ready: bool,
    pub success: bool,
    pub error: Option<InstallError>,
}

impl MemTransport {
    pub fn for_papk(papk: &[u8]) -> Self {
        let mut h = pdb_protocol::Crc32::new();
        h.update(&[pdb_protocol::CMD_INSTALL]);
        h.update(&(papk.len() as u32).to_le_bytes());
        h.update(papk);
        let mut data = papk.to_vec();
        data.extend_from_slice(&h.finalize().to_le_bytes());
        Self {
            data,
            pos: 0,
            ready: false,
            success: false,
            error: None,
        }
    }
}

impl InstallTransport for MemTransport {
    fn read_byte(&mut self) -> Result<u8, ReadError> {
        let b = *self.data.get(self.pos).ok_or(ReadError::Timeout)?;
        self.pos += 1;
        Ok(b)
    }
    fn report_ready(&mut self) {
        self.ready = true;
    }
    fn report_success(&mut self) {
        self.success = true;
    }
    fn report_error(&mut self, error: InstallError) {
        self.error = Some(error);
    }
}

/// No JVM core to park: the simulator services installs on the JVM task
/// itself, and the tests have none.
pub struct NoCoordinator;

impl CoreCoordinator for NoCoordinator {
    fn request_stop_and_park(&mut self) {}
    fn wait_for_park(&mut self) -> bool {
        true
    }
    fn release(&mut self) {}
    fn cancel_park_request(&mut self) {}
}
