// SPDX-License-Identifier: GPL-3.0-only
//! This family's half of PAPK install.
//!
//! The orchestration — validate, park, erase, stream, verify, commit — is
//! `picodroid_core::install`. What is left here is the flash slot itself:
//! where it sits, how it erases, and the linker section probe-rs writes when
//! it flashes an ELF.

#[cfg(not(any(test, feature = "sim")))]
pub mod flash;

#[cfg(not(any(test, feature = "sim")))]
pub use rp_flash::RpPapkFlash;

#[cfg(not(any(test, feature = "sim")))]
mod rp_flash {
    use picodroid_core::install::PapkFlash;

    /// This family's PAPK slot, as `picodroid_core::install` sees it.
    ///
    /// Zero-sized: the slot is a fixed region named by linker symbols and
    /// chip-gated constants, not something with instance state. It exists to
    /// carry the trait.
    pub struct RpPapkFlash;

    // SAFETY: every method delegates to `hal::flash`, whose erase/program
    // primitives disable XIP for the duration of the ROM call and run from
    // RAM. `run_install` parks the JVM core before reaching any of them,
    // which is the condition the trait documents.
    unsafe impl PapkFlash for RpPapkFlash {
        fn max_data_size(&self) -> usize {
            super::flash::PAPK_MAX_DATA_SIZE
        }

        unsafe fn erase_region(&mut self, papk_len: usize) {
            super::flash::flash_erase_papk_region(papk_len)
        }

        unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool {
            super::flash::flash_write_page(page_index, page)
        }

        unsafe fn commit_metadata(&mut self, len: u32) {
            super::flash::flash_commit_metadata(len)
        }

        fn trigger_reset(&mut self) -> ! {
            super::flash::flash_trigger_reset()
        }
    }
}
