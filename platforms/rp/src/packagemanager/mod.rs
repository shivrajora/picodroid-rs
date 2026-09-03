// SPDX-License-Identifier: GPL-3.0-only
//! This family's half of PAPK install.
//!
//! The orchestration — validate, park, erase, stream, verify, commit — is
//! `picodroid_core::install`, and the slot arithmetic on top of the flash
//! primitives is its `PapkSlot`. What is left here is what only this family
//! can say: where the slot sits, how big it is, how a range is erased and
//! programmed, how the chip resets, and the linker section probe-rs writes
//! when it flashes an ELF.

#[cfg(not(any(test, feature = "sim")))]
pub mod flash;

#[cfg(not(any(test, feature = "sim")))]
pub use rp_flash::RpPapkFlash;

#[cfg(not(any(test, feature = "sim")))]
mod rp_flash {
    use picodroid_core::install::{PapkSlot, PapkSlotFlash};

    /// This family's PAPK slot primitives: a fixed region named by chip-gated
    /// constants, erased and programmed by the ROM routines in `hal::flash`.
    pub struct RpFlash;

    // SAFETY: every primitive delegates to `hal::flash`, whose erase/program
    // routines disable XIP for the duration of the ROM call and run from RAM.
    // `run_install` parks the JVM core before reaching any of them, which is
    // the condition the trait documents.
    unsafe impl PapkSlotFlash for RpFlash {
        const META_OFFSET: u32 = super::flash::PAPK_FLASH_META_OFFSET;
        const MAX_DATA_SIZE: usize = super::flash::PAPK_MAX_DATA_SIZE;
        const SECTOR_SIZE: usize = super::flash::FLASH_SECTOR_SIZE;

        unsafe fn erase_range(flash_offset: u32, len: usize) {
            super::flash::flash_erase_range(flash_offset, len)
        }

        unsafe fn program_range(flash_offset: u32, data: &[u8]) {
            super::flash::flash_program_range(flash_offset, data.as_ptr(), data.len())
        }

        fn reset() -> ! {
            super::flash::flash_trigger_reset()
        }
    }

    /// This family's slot, as `picodroid_core::install` sees it.
    pub type RpPapkFlash = PapkSlot<RpFlash>;
}
