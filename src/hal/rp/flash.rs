// ── Flash constants ───────────────────────────────────────────────────────────

// RP2040 flash-relative offsets (relative to flash XIP base 0x10000000)
#[cfg(feature = "chip-rp2040")]
pub const PAPK_FLASH_XIP_BASE: usize = 0x1010_0000;
#[cfg(feature = "chip-rp2040")]
pub const PAPK_FLASH_META_OFFSET: u32 = 0x0010_0000; // for ROM erase/program calls

// RP2350 flash-relative offsets (4 MB flash, last 1 MB)
#[cfg(feature = "chip-rp2350-hal")]
pub const PAPK_FLASH_XIP_BASE: usize = 0x1030_0000;
#[cfg(feature = "chip-rp2350-hal")]
pub const PAPK_FLASH_META_OFFSET: u32 = 0x0030_0000;

pub const PAPK_BOOT_META_SIZE: usize = 4096; // one 4 KB erase sector
const PAPK_SLOT_OFFSET_FROM_META: usize = PAPK_BOOT_META_SIZE;

pub const PAPK_FLASH_MAGIC: u32 = 0x5044_4231; // "PDB1"
pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024; // 1020 KB (1 MB slot minus 4 KB metadata sector)

/// XIP base address for both supported chips.
pub const XIP_BASE: usize = 0x1000_0000;

/// Flash erase sector size (RP2040/RP2350 NOR flash).
pub const FLASH_SECTOR_SIZE: usize = 4096;

/// Flash program page size.
pub const FLASH_PAGE_SIZE: usize = 256;

// FS region symbols defined in the linker script (mcus/rp/rp20{4,35}0.x).
extern "C" {
    static __fs_start: u8;
    static __fs_end: u8;
}

/// Returns (start_offset, length_bytes) of the LittleFS region, relative to
/// the flash XIP base (0x10000000).  Both are 4 KB-aligned.
pub fn fs_region_bounds() -> (u32, u32) {
    let start = (&raw const __fs_start) as usize;
    let end = (&raw const __fs_end) as usize;
    debug_assert!(start >= XIP_BASE && end > start);
    debug_assert!(start.is_multiple_of(FLASH_SECTOR_SIZE));
    debug_assert!(end.is_multiple_of(FLASH_SECTOR_SIZE));
    ((start - XIP_BASE) as u32, (end - start) as u32)
}

/// Read from an XIP-mapped flash offset (no ROM call; XIP must be enabled).
///
/// # Safety
/// Caller must ensure `flash_offset + buf.len() <= flash size`, XIP is
/// currently enabled, and no concurrent erase/program is in flight.
pub unsafe fn flash_read(flash_offset: u32, buf: &mut [u8]) {
    let src = (XIP_BASE + flash_offset as usize) as *const u8;
    core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len());
}

// ── Read ─────────────────────────────────────────────────────────────────────

/// Check the PAPK flash region for a valid persistent install.
///
/// The returned slice is `'static` because it points directly into XIP-mapped
/// flash — no copy required, and `Jvm::load_class` accepts it.
///
/// # Safety
/// Must only be called before the FreeRTOS scheduler starts (single-core,
/// no concurrent flash writes possible).
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

// ── Flash operations (XIP-disabled) ──────────────────────────────────────────
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
        #[cfg(feature = "chip-rp2350-hal")]
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

// ── Generic absolute-address primitives ──────────────────────────────────────
//
// These take flash-relative offsets (0 = XIP base) and are the single point
// where XIP is disabled.  PAPK and LittleFS helpers sit on top of them.

/// Erase `len` bytes of flash starting at `flash_offset` (both 4 KB-aligned).
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn flash_erase_range(flash_offset: u32, len: usize) {
    with_xip_disabled!(flash_range_erase, |erase| {
        erase(flash_offset, len, FLASH_SECTOR_SIZE as u32, 0x20)
    });
}

/// Program `data` into flash at `flash_offset`.  Both the offset and `data.len()`
/// must be multiples of `FLASH_PAGE_SIZE` (256 bytes).
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn flash_program_range(flash_offset: u32, data: *const u8, len: usize) {
    with_xip_disabled!(flash_range_program, |program| {
        program(flash_offset, data, len)
    });
}

// ── PAPK helpers (layered on the primitives above) ───────────────────────────

/// Erase the flash sectors needed to hold `papk_len` bytes plus the metadata sector.
pub unsafe fn flash_erase_papk_region(papk_len: usize) {
    let data_erase = papk_len.div_ceil(FLASH_SECTOR_SIZE) * FLASH_SECTOR_SIZE;
    let total_erase = PAPK_BOOT_META_SIZE + data_erase;
    flash_erase_range(PAPK_FLASH_META_OFFSET, total_erase);
}

/// Write one 256-byte flash page into the PAPK data region.
pub unsafe fn flash_write_page(page_index: u32, data: &[u8; 256]) -> bool {
    let offset_within_slot = page_index as usize * 256;
    if offset_within_slot + 256 > PAPK_MAX_DATA_SIZE {
        return false;
    }
    let flash_offset =
        PAPK_FLASH_META_OFFSET + PAPK_BOOT_META_SIZE as u32 + offset_within_slot as u32;
    flash_program_range(flash_offset, data.as_ptr(), 256);
    true
}

/// Write the PapkBootMeta header to sector 0, committing the install atomically.
pub unsafe fn flash_commit_metadata(len: u32) {
    let mut meta_page = [0xFFu8; 256];
    meta_page[0..4].copy_from_slice(&PAPK_FLASH_MAGIC.to_le_bytes());
    meta_page[4..8].copy_from_slice(&0u32.to_le_bytes()); // flags: slot=A, unverified
    meta_page[8..12].copy_from_slice(&len.to_le_bytes());
    flash_program_range(PAPK_FLASH_META_OFFSET, meta_page.as_ptr(), 256);
}

// ── Park ─────────────────────────────────────────────────────────────────────

/// Park the calling core in a RAM spin loop with interrupts disabled.
///
/// Called from jvm_task (core 0) so that core 0 does not access flash (via XIP)
/// while the install task (core 1) erases or programs flash.
///
/// # Safety
/// Must only be called from core 0 after the JVM and all child threads have
/// exited.  Core 1 must eventually set `CORE0_RELEASE` to avoid permanent
/// lockup.
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn park_for_flash() {
    use crate::pdb::pending;

    // Pre-compute flag addresses while XIP is still enabled
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
    core::arch::asm!(
        "dmb sy",
        "strb {val}, [{ptr}]",
        "sev",
        val = in(reg) 1u32,
        ptr = in(reg) parked_addr,
        options(nostack, preserves_flags)
    );

    // Spin until core 1 sets CORE0_RELEASE.
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
        core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
    }

    // Clear all flags
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

// ── Reset ────────────────────────────────────────────────────────────────────

/// Trigger a full chip reset via the RP2040/RP2350 watchdog.
pub fn flash_trigger_reset() -> ! {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let p = unsafe { pac::Peripherals::steal() };

    // Tell the PSM to reset every subsystem except the ring and crystal
    // oscillators when the watchdog fires.
    p.PSM.wdsel().write(|w| unsafe { w.bits(0x0001_fffc) });

    // Force-fire the watchdog (CTRL bit 31 = TRIGGER).
    p.WATCHDOG.ctrl().write(|w| unsafe { w.bits(1 << 31) });

    loop {
        cortex_m::asm::nop();
    }
}
