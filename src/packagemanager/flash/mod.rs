// Embed the PAPK flash initialiser section.
// build.rs generates papk_flash_init.rs which declares a static array placed
// in the .papk_flash_init section at the PAPK_FLASH address.  probe-rs writes
// this section when flashing the ELF, so the persistent PAPK region is always
// updated to match the baked-in APK on every probe flash.
#[cfg(not(any(test, feature = "sim")))]
include!(concat!(env!("OUT_DIR"), "/papk_flash_init.rs"));

// Persistent PAPK flash region layout:
//
//   Sector 0  [4KB]:   PapkBootMeta { magic: u32, flags: u32, len: u32, _pad: [u8; 4084] }
//   Remaining [1020KB]: raw PAPK bytes
//
// The PapkBootMeta header is written LAST (after all PAPK data), acting as an
// atomic commit marker — if power is lost mid-write the magic stays invalid and
// the device boots the baked-in APK instead.
//
// flags bit 0 = active_slot (reserved for future A/B support, always 0 today).
// flags bit 1 = verified    (reserved for future watchdog-rollback support).
//
// Future A/B path: expand PAPK_FLASH to 2 MB, add Slot B after Slot A,
// flip `flags & 1` after writing the inactive slot — no other caller changes.

// RP2040 flash-relative offsets (relative to flash XIP base 0x10000000)
#[cfg(feature = "chip-rp2040")]
const PAPK_FLASH_XIP_BASE: usize = 0x1010_0000;
#[cfg(feature = "chip-rp2040")]
const PAPK_FLASH_META_OFFSET: u32 = 0x0010_0000; // for ROM erase/program calls

// RP2350 flash-relative offsets (4 MB flash, last 1 MB)
#[cfg(feature = "chip-rp2350")]
const PAPK_FLASH_XIP_BASE: usize = 0x1030_0000;
#[cfg(feature = "chip-rp2350")]
const PAPK_FLASH_META_OFFSET: u32 = 0x0030_0000;

const PAPK_BOOT_META_SIZE: usize = 4096; // one 4 KB erase sector
const PAPK_SLOT_OFFSET_FROM_META: usize = PAPK_BOOT_META_SIZE;

pub const PAPK_FLASH_MAGIC: u32 = 0x5044_4231; // "PDB1"
pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024; // 1020 KB (1 MB slot minus 4 KB metadata sector)

/// Check the PAPK flash region for a valid persistent install.
///
/// The returned slice is `'static` because it points directly into XIP-mapped
/// flash — no copy required, and `Jvm::load_class` accepts it.
///
/// # Safety
/// Must only be called before the FreeRTOS scheduler starts (single-core,
/// no concurrent flash writes possible).
#[cfg(not(feature = "sim"))]
pub unsafe fn read_flash_papk() -> Option<&'static [u8]> {
    let meta_ptr = PAPK_FLASH_XIP_BASE as *const u32;
    let magic = *meta_ptr;
    if magic != PAPK_FLASH_MAGIC {
        return None;
    }
    // flags word at offset 4 (reserved for A/B; skip for now)
    let len = *(meta_ptr.add(2)) as usize; // offset 8
    if len == 0 || len > PAPK_MAX_DATA_SIZE {
        return None;
    }
    let data_ptr = (PAPK_FLASH_XIP_BASE + PAPK_SLOT_OFFSET_FROM_META) as *const u8;
    Some(core::slice::from_raw_parts(data_ptr, len))
}

mod ops;
mod park;
mod reset;

pub use ops::{flash_commit_metadata, flash_erase_papk_region, flash_write_page};
pub use park::park_for_flash;
pub use reset::flash_trigger_reset;
