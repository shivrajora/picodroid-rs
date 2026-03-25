// Persistent PAPK flash region layout:
//
//   Sector 0  [4KB]:  PapkBootMeta { magic: u32, flags: u32, len: u32, _pad: [u8; 4084] }
//   Remaining [124KB]: raw PAPK bytes
//
// The PapkBootMeta header is written LAST (after all PAPK data), acting as an
// atomic commit marker — if power is lost mid-write the magic stays invalid and
// the device boots the baked-in APK instead.
//
// flags bit 0 = active_slot (reserved for future A/B support, always 0 today).
// flags bit 1 = verified    (reserved for future watchdog-rollback support).
//
// Future A/B path: expand PAPK_FLASH to 256 KB, add Slot B after Slot A,
// flip `flags & 1` after writing the inactive slot — no other caller changes.

// RP2040 flash-relative offsets (relative to flash XIP base 0x10000000)
#[cfg(feature = "chip-rp2040")]
const PAPK_FLASH_XIP_BASE: usize = 0x101E_0000;
#[cfg(feature = "chip-rp2040")]
const PAPK_FLASH_META_OFFSET: u32 = 0x001E_0000; // for ROM erase/program calls

// RP2350 flash-relative offsets (4 MB flash, last 128 KB)
#[cfg(feature = "chip-rp2350")]
const PAPK_FLASH_XIP_BASE: usize = 0x103E_0000;
#[cfg(feature = "chip-rp2350")]
const PAPK_FLASH_META_OFFSET: u32 = 0x003E_0000;

const PAPK_BOOT_META_SIZE: usize = 4096; // one 4 KB erase sector
const PAPK_SLOT_OFFSET_FROM_META: usize = PAPK_BOOT_META_SIZE;

pub const PAPK_FLASH_MAGIC: u32 = 0x5044_4231; // "PDB1"
pub const PAPK_MAX_DATA_SIZE: usize = 124 * 1024; // 124 KB

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

/// Erase the PAPK flash region and write a new PAPK.
///
/// Write order: data first, then metadata header — so an interrupted write
/// leaves the magic invalid and the device falls back to the baked-in APK.
///
/// This function MUST be linked to RAM (`#[link_section = ".data"]`) because
/// the RP2040/RP2350 disables the XIP cache during flash erase/program.
/// `cortex_m::interrupt::free` disables all interrupts for the duration.
///
/// Returns `false` if `data` is too large for the slot.
#[cfg(not(feature = "sim"))]
#[link_section = ".data"]
pub unsafe fn write_papk_to_flash(data: &[u8]) -> bool {
    if data.len() > PAPK_MAX_DATA_SIZE {
        return false;
    }

    // Erase: meta sector (4 KB) + enough data sectors (4 KB aligned)
    let data_erase = (data.len() + 4095) & !4095;
    let total_erase = PAPK_BOOT_META_SIZE + data_erase;

    cortex_m::interrupt::free(|_| {
        #[cfg(feature = "chip-rp2350")]
        use rp235x_hal::rom_data;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::rom_data;

        // 1. Erase everything (meta + data sectors)
        rom_data::flash_range_erase(
            PAPK_FLASH_META_OFFSET,
            total_erase,
            1 << 16, // 64 KB block erase command size
            0xd8,    // block erase command (standard SPI flash)
        );

        // 2. Program PAPK data in 256-byte pages (before writing meta)
        let data_flash_offset = PAPK_FLASH_META_OFFSET + PAPK_BOOT_META_SIZE as u32;
        let mut written = 0usize;
        while written < data.len() {
            let chunk_end = (written + 256).min(data.len());
            let mut page = [0xFFu8; 256];
            page[..chunk_end - written].copy_from_slice(&data[written..chunk_end]);
            rom_data::flash_range_program(data_flash_offset + written as u32, page.as_ptr(), 256);
            written += 256;
        }

        // 3. Write boot metadata last (atomic commit)
        //    Only the first 256-byte page of the 4 KB meta sector is used.
        let mut meta_page = [0xFFu8; 256];
        meta_page[0..4].copy_from_slice(&PAPK_FLASH_MAGIC.to_le_bytes());
        meta_page[4..8].copy_from_slice(&0u32.to_le_bytes()); // flags: slot=A, unverified
        meta_page[8..12].copy_from_slice(&(data.len() as u32).to_le_bytes());
        rom_data::flash_range_program(PAPK_FLASH_META_OFFSET, meta_page.as_ptr(), 256);
    });

    true
}
