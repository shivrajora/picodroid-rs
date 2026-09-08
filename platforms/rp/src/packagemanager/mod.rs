// SPDX-License-Identifier: GPL-3.0-only
//! This family's half of PAPK install.
//!
//! The orchestration — validate, park, place, erase, stream, verify, commit —
//! is `picodroid_core::install`, and the run arithmetic on top of the flash
//! primitives is its `PapkRegion`. What is left here is what only this
//! family can say: where the app region sits, how big it is, where it is
//! mapped, how a range is erased and programmed, how the chip resets, and
//! the linker section probe-rs writes when it flashes an ELF.

#[cfg(not(any(test, feature = "sim")))]
pub mod flash;

#[cfg(not(any(test, feature = "sim")))]
pub use rp_flash::RpPapkFlash;

#[cfg(not(any(test, feature = "sim")))]
mod rp_flash {
    use picodroid_core::install::{PapkRegion, PapkRegionFlash};

    /// This family's app-region primitives: the region the generated flash
    /// layout names, erased and programmed by the ROM routines in
    /// `hal::flash`, read through XIP.
    pub struct RpFlash;

    // SAFETY: every primitive delegates to `hal::flash`, whose erase/program
    // routines disable XIP for the duration of the ROM call and run from RAM.
    // `run_install` parks the JVM core before reaching any of them, which is
    // the condition the trait documents; the mapped base is XIP flash, which
    // stays readable for the life of the firmware.
    unsafe impl PapkRegionFlash for RpFlash {
        const REGION_OFFSET: u32 = super::flash::PAPK_REGION_OFFSET;
        const REGION_LEN: usize = super::flash::PAPK_REGION_LEN;
        const SECTOR_SIZE: usize = super::flash::FLASH_SECTOR_SIZE;
        const MAX_INSTALLED_APPS: usize = super::flash::MAX_INSTALLED_APPS;

        fn mapped_base() -> *const u8 {
            super::flash::region_base()
        }

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

    /// This family's region, as `picodroid_core::install` sees it.
    pub type RpPapkFlash = PapkRegion<RpFlash>;
}
