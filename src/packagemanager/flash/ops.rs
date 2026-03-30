// ── Flash operation helpers ───────────────────────────────────────────────────
//
// All functions below are placed in `.data` (RAM) and follow these rules:
//
// 1. NO defmt calls anywhere — defmt thunks (Thumbv6MABSLongThunk) call into
//    .text (flash) via `bl`, which hard-faults once XIP is disabled.
//
// 2. All ROM function pointers are pre-resolved via `rom_data::X::ptr()` while
//    XIP is still enabled, stored as locals, then invoked via `blx rN`.  The
//    `ptr()` wrappers live in .text but are called BEFORE `cpsid i`.
//
// 3. XIP sequence for flash operations (per the RP2040 pico-sdk):
//      connect_internal_flash()  — restores QSPI pad controls, connects SSI
//      flash_exit_xip()          — configures SSI for serial SPI mode, exits XIP
//      <erase or program>        — performs the flash operation
//      flash_flush_cache()       — flushes XIP cache, clears CS IO-forcing
//      flash_enter_cmd_xip()     — restores 03h-command XIP mode
//    The raw ROM functions (flash_range_erase, flash_range_program) do NOT
//    handle XIP internally — the pico-sdk C wrappers perform this sequence.
//
// 4. park_for_flash (core 0) uses read_volatile / write_volatile for all flag
//    accesses, NOT AtomicBool::load/store.  On thumbv6m, AtomicBool operations
//    may lower to __atomic_load_1 / __atomic_store_1 libcalls (in .text flash).
//    Those libcalls fault when core 1's flash_exit_xip has disabled XIP.
//    read_volatile/write_volatile lower to single ldrb/strb instructions only.

/// Execute a flash operation with XIP disabled.
///
/// This macro:
/// 1. Imports `rom_data` (chip-cfg'd) and pre-resolves all ROM function pointers
/// 2. Saves PRIMASK and disables interrupts
/// 3. Runs: connect → exit_xip → caller's operation → flush → enter_xip
/// 4. Restores interrupts if they were previously enabled
///
/// # Why a macro?
/// Any helper called from a `#[link_section = ".data"]` function must be
/// guaranteed to expand inline.  A macro provides this guarantee — unlike
/// `#[inline(always)]`, which is only a compiler hint.  A non-inlined call
/// would jump into .text (flash), causing a hard-fault once XIP is disabled.
macro_rules! with_xip_disabled {
    ($op_fn:ident, |$op:ident| $body:expr) => {{
        #[cfg(feature = "chip-rp2350")]
        use rp235x_hal::rom_data;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::rom_data;

        // Pre-resolve all ROM function pointers while XIP is still enabled.
        let connect = rom_data::connect_internal_flash::ptr();
        let exit_xip = rom_data::flash_exit_xip::ptr();
        let $op = rom_data::$op_fn::ptr();
        let flush = rom_data::flash_flush_cache::ptr();
        let enter_xip = rom_data::flash_enter_cmd_xip::ptr();

        let primask: u32;
        core::arch::asm!(
            "mrs {}, PRIMASK",
            out(reg) primask,
            options(nomem, nostack, preserves_flags)
        );
        core::arch::asm!("cpsid i", options(nomem, nostack));

        connect();
        exit_xip();
        $body;
        flush();
        enter_xip();

        if primask == 0 {
            core::arch::asm!("cpsie i", options(nomem, nostack));
        }
    }};
}

/// Erase the flash sectors needed to hold `papk_len` bytes plus the metadata sector.
///
/// Only erases what is necessary:
///   - always erases the 4 KB metadata sector (sector 0 of the slot)
///   - erases enough 4 KB data sectors to hold `papk_len` bytes
///
/// Using 4 KB sector erase (0x20) rather than 64 KB block erase (0xd8) lets
/// small apps erase only a few sectors instead of the entire 1 MB slot.
///
/// Must be called before streaming page writes. XIP is disabled for the
/// duration so this function MUST run from RAM (`#[link_section = ".data"]`).
/// Interrupts are disabled on the calling core for the entire erase.
#[cfg(not(feature = "sim"))]
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn flash_erase_papk_region(papk_len: usize) {
    const SECTOR: usize = 4096;
    let data_erase = papk_len.div_ceil(SECTOR) * SECTOR;
    let total_erase = super::PAPK_BOOT_META_SIZE + data_erase;

    with_xip_disabled!(flash_range_erase, |erase| {
        erase(
            super::PAPK_FLASH_META_OFFSET,
            total_erase,
            SECTOR as u32,
            0x20,
        )
    });
}

/// Write one 256-byte flash page into the PAPK data region.
///
/// `page_index` is 0-based; `data` must be exactly 256 bytes (pad with 0xFF
/// for the final partial page).  Returns `false` if `page_index` is out of
/// range for the 124 KB slot.
///
/// MUST run from RAM (`#[link_section = ".data"]`). Interrupts are disabled
/// for the duration (~1 ms).
#[cfg(not(feature = "sim"))]
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn flash_write_page(page_index: u32, data: &[u8; 256]) -> bool {
    let offset_within_slot = page_index as usize * 256;
    // Reject any page whose 256-byte write would extend past the data region.
    if offset_within_slot + 256 > super::PAPK_MAX_DATA_SIZE {
        return false;
    }

    let flash_offset = super::PAPK_FLASH_META_OFFSET
        + super::PAPK_BOOT_META_SIZE as u32
        + offset_within_slot as u32;

    with_xip_disabled!(flash_range_program, |program| {
        program(flash_offset, data.as_ptr(), 256)
    });

    true
}

/// Write the PapkBootMeta header to sector 0, committing the install atomically.
///
/// Call this AFTER all data pages have been written successfully and CRC has
/// been verified.  Writing the magic last ensures that an interrupted write
/// leaves the slot invalid, causing a fallback to the baked-in APK on boot.
///
/// MUST run from RAM (`#[link_section = ".data"]`). Interrupts are disabled
/// for the duration (~1 ms).
#[cfg(not(feature = "sim"))]
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn flash_commit_metadata(len: u32) {
    let mut meta_page = [0xFFu8; 256];
    meta_page[0..4].copy_from_slice(&super::PAPK_FLASH_MAGIC.to_le_bytes());
    meta_page[4..8].copy_from_slice(&0u32.to_le_bytes()); // flags: slot=A, unverified
    meta_page[8..12].copy_from_slice(&len.to_le_bytes());

    with_xip_disabled!(flash_range_program, |program| {
        program(super::PAPK_FLASH_META_OFFSET, meta_page.as_ptr(), 256)
    });
}
