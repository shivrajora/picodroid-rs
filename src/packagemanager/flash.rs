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
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::rom_data;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::rom_data;

    const SECTOR: usize = 4096;
    let data_erase = papk_len.div_ceil(SECTOR) * SECTOR;
    let total_erase = PAPK_BOOT_META_SIZE + data_erase;

    // Pre-resolve all ROM function pointers while XIP is still enabled.
    let connect = rom_data::connect_internal_flash::ptr();
    let exit_xip = rom_data::flash_exit_xip::ptr();
    let erase = rom_data::flash_range_erase::ptr();
    let flush = rom_data::flash_flush_cache::ptr();
    let enter_xip = rom_data::flash_enter_cmd_xip::ptr();

    let primask: u32;
    core::arch::asm!("mrs {}, PRIMASK", out(reg) primask, options(nomem, nostack, preserves_flags));
    core::arch::asm!("cpsid i", options(nomem, nostack));

    connect();
    exit_xip();
    erase(PAPK_FLASH_META_OFFSET, total_erase, SECTOR as u32, 0x20);
    flush();
    enter_xip();

    if primask == 0 {
        core::arch::asm!("cpsie i", options(nomem, nostack));
    }
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
    if offset_within_slot + 256 > PAPK_MAX_DATA_SIZE {
        return false;
    }

    let flash_offset =
        PAPK_FLASH_META_OFFSET + PAPK_BOOT_META_SIZE as u32 + offset_within_slot as u32;

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::rom_data;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::rom_data;

    let connect = rom_data::connect_internal_flash::ptr();
    let exit_xip = rom_data::flash_exit_xip::ptr();
    let program = rom_data::flash_range_program::ptr();
    let flush = rom_data::flash_flush_cache::ptr();
    let enter_xip = rom_data::flash_enter_cmd_xip::ptr();

    let primask: u32;
    core::arch::asm!("mrs {}, PRIMASK", out(reg) primask, options(nomem, nostack, preserves_flags));
    core::arch::asm!("cpsid i", options(nomem, nostack));

    connect();
    exit_xip();
    program(flash_offset, data.as_ptr(), 256);
    flush();
    enter_xip();

    if primask == 0 {
        core::arch::asm!("cpsie i", options(nomem, nostack));
    }

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
    meta_page[0..4].copy_from_slice(&PAPK_FLASH_MAGIC.to_le_bytes());
    meta_page[4..8].copy_from_slice(&0u32.to_le_bytes()); // flags: slot=A, unverified
    meta_page[8..12].copy_from_slice(&len.to_le_bytes());

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::rom_data;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::rom_data;

    let connect = rom_data::connect_internal_flash::ptr();
    let exit_xip = rom_data::flash_exit_xip::ptr();
    let program = rom_data::flash_range_program::ptr();
    let flush = rom_data::flash_flush_cache::ptr();
    let enter_xip = rom_data::flash_enter_cmd_xip::ptr();

    let primask: u32;
    core::arch::asm!("mrs {}, PRIMASK", out(reg) primask, options(nomem, nostack, preserves_flags));
    core::arch::asm!("cpsid i", options(nomem, nostack));

    connect();
    exit_xip();
    program(PAPK_FLASH_META_OFFSET, meta_page.as_ptr(), 256);
    flush();
    enter_xip();

    if primask == 0 {
        core::arch::asm!("cpsie i", options(nomem, nostack));
    }
}

/// Park the calling core in a RAM spin loop with interrupts disabled.
///
/// Called from jvm_task (core 0) so that core 0 does not access flash (via XIP)
/// while the install task (core 1) erases or programs flash.  The RP2040 SSI is
/// shared between both cores; any XIP access during a ROM flash operation causes
/// undefined behaviour (bus hang or hard-fault).
///
/// Core 0 stays parked until the install coordinator sets
/// [`crate::pdb::pending::CORE0_RELEASE`].
///
/// # Safety
/// Must only be called from core 0 after the JVM and all child threads have
/// exited.  Core 1 must eventually set `CORE0_RELEASE` to avoid permanent
/// lockup.
#[cfg(not(feature = "sim"))]
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn park_for_flash() {
    use crate::pdb::pending;

    // Pre-compute flag addresses while XIP is still enabled (as_ptr() may not
    // be inlined in debug builds; calling before cpsid i is safe).
    let parked_addr = pending::CORE0_PARKED.as_ptr() as usize;
    let release_addr = pending::CORE0_RELEASE.as_ptr() as usize;
    let park_req_addr = pending::FLASH_PARK_REQUESTED.as_ptr() as usize;

    let primask: u32;
    core::arch::asm!(
        "mrs {0}, PRIMASK",
        out(reg) primask,
        options(nomem, nostack, preserves_flags)
    );
    core::arch::asm!("cpsid i", options(nomem, nostack));

    // Signal core 1 we are parked.
    //
    // Use raw strb/ldrb inline asm — read_volatile/write_volatile are only
    // #[inline] (not #[inline(always)]), so in debug builds they are NOT
    // inlined and call into .text (flash).  That crashes once core 1's
    // flash_exit_xip has disabled XIP.  Inline asm is guaranteed to be a
    // single strb/ldrb with no function call.
    //
    // dmb before strb = Release fence; ldrb then dmb = Acquire load.
    core::arch::asm!(
        "dmb sy",
        "strb {val}, [{ptr}]",
        val = in(reg) 1u32,
        ptr = in(reg) parked_addr,
        options(nostack, preserves_flags)
    );

    // Spin until core 1 sets CORE0_RELEASE.
    // NO `readonly` — that would let the compiler hoist the load out of the
    // loop, reading CORE0_RELEASE only once.  Without `readonly` the asm is
    // treated as a memory side-effect and re-executed on every iteration.
    loop {
        let val: u32;
        core::arch::asm!(
            "ldrb {val}, [{ptr}]",
            "dmb sy",
            val = out(reg) val,
            ptr = in(reg) release_addr,
            options(nostack, preserves_flags)
        );
        if val != 0 {
            break;
        }
        // Use inline asm, NOT cortex_m::asm::nop() — the cortex-m crate's
        // nop() calls `__nop` via a thunk into flash, which faults once
        // core 1 has disabled XIP.
        core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
    }

    // Clear all flags (interrupts still off, no reorder risk).
    core::arch::asm!(
        "strb {z}, [{p1}]",
        "strb {z}, [{p2}]",
        "strb {z}, [{p3}]",
        z  = in(reg) 0u32,
        p1 = in(reg) parked_addr,
        p2 = in(reg) release_addr,
        p3 = in(reg) park_req_addr,
        options(nostack, preserves_flags)
    );

    if primask == 0 {
        core::arch::asm!("cpsie i", options(nomem, nostack));
    }
}

/// Trigger a full chip reset via the RP2040/RP2350 watchdog.
///
/// Both cores reset.  The bootloader re-runs, then `main()` starts fresh,
/// loading the newly-installed PAPK from flash via XIP.
///
/// Uses the watchdog TRIGGER path through the PSM (Power-on State Machine)
/// instead of SYSRESETREQ.  SYSRESETREQ (AIRCR bit 2) is not guaranteed to
/// propagate to a full chip reset on multi-core RP2040/RP2350 — the watchdog
/// path is what the pico-sdk uses and resets all selected subsystems reliably.
///
/// Called from the install task (core 1) after the install is complete.
#[cfg(not(feature = "sim"))]
pub fn flash_trigger_reset() -> ! {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let p = unsafe { pac::Peripherals::steal() };

    // Tell the PSM to reset every subsystem except the ring and crystal
    // oscillators when the watchdog fires.  WDSEL resets to 0x0000_0000
    // (nothing selected), so we must set it explicitly.
    // Bits: 0=ROSC, 1=XOSC, 2..16=everything else.
    p.PSM.wdsel().write(|w| unsafe { w.bits(0x0001_fffc) });

    // Force-fire the watchdog (CTRL bit 31 = TRIGGER).
    p.WATCHDOG.ctrl().write(|w| unsafe { w.bits(1 << 31) });

    loop {
        cortex_m::asm::nop();
    }
}
