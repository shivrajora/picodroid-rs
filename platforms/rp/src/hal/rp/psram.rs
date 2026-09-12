// SPDX-License-Identifier: GPL-3.0-only
//! The QSPI PSRAM on the module's second chip select.
//!
//! spin-ok-file: the QMI direct-mode exchanges below poll CSR bits for a
//! few bus cycles each, once, before the scheduler exists (`main.rs` runs
//! [`init`] ahead of `boot_tasks::start_tasks`); nothing can be starved.
//!
//! An APS6404L-class part — 8 MB on the Pimoroni Pico Plus 2 W — hangs off
//! the same QSPI pads as the flash, selected by XIP_CS1n on the pad the MCU
//! toml names (`psram_cs_pin`), and is reached through the QMI's memory
//! window 1: [`PSRAM_ORIGIN`] (`psram_origin`, 0x11000000), [`PSRAM_LEN`]
//! bytes. The bootrom leaves that window alone, so [`init`] does at boot what
//! pico-sdk's `hardware_psram` does at runtime init: put the pad on XIP_CS1n,
//! talk to the part through the QMI's direct mode to read its ID and switch
//! it to QPI, then program window 1's timing and read/write formats and make
//! the window writable. From then on it is memory at [`PSRAM_ORIGIN`].
//!
//! # It is not SRAM
//! It is reached over the same QSPI bus and the same 16 KB XIP cache as the
//! flash the code runs from, at a 75 MHz bus clock and with a cache miss
//! costing on the order of a hundred cycles. What may live here is cold and
//! large: the LVGL pool (board.toml `lv_mem_in_psram`) is the first tenant,
//! and even it keeps its render targets in SRAM (`lv_draw_buf_sram.c`).
//! Never the JVM arena, the operand stacks or the draw band — the
//! interpreter touches the first two every opcode and the renderer the last
//! every pixel (docs/designs/psram-lvgl-fluid-scroll-2026-09.md).
//!
//! # The XIP rule
//! PSRAM sits behind the same XIP window that runtime flash writes switch
//! off. `with_xip_disabled!` (flash.rs) owes it three things: write back
//! every dirty cache line before the ROM's `flash_flush_cache` invalidates the
//! cache — the RP2350's XIP cache is write-back for this window —, save
//! window 1's registers before `flash_exit_xip` resets them to the ROM's
//! serial-read defaults, and write them back after. Between XIP-off and
//! XIP-restore no code on either core may touch PSRAM; core 1 is parked and
//! the calling core has interrupts masked, which is the same discipline that
//! keeps instruction fetches out of the window. The contents survive: the
//! part self-refreshes while deselected. What the simulator models of any of
//! this is nothing, so a violation is a hardware-only bug — the on-device
//! test is an install (real erases and programs) while the UI is live.

use rp235x_hal::pac;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/psram_config.rs"));
}
pub use super::flash::{PSRAM_LEN, PSRAM_ORIGIN};

/// The XIP space's cached base, its uncached non-allocating alias, and its
/// cache-maintenance alias (RP2350 datasheet §2.2, §4.4.1). Window 1 sits
/// at the same offset in each.
const XIP_BASE: usize = 0x1000_0000;
const XIP_NOCACHE_NOALLOC_BASE: usize = 0x1400_0000;
const XIP_MAINTENANCE_BASE: usize = 0x1800_0000;
const XIP_END: usize = 0x1400_0000;
/// The XIP cache: 16 KB, 8-byte lines, write-back for window 1.
pub const XIP_CACHE_SIZE: usize = 16 * 1024;
/// The uncached alias of a cached XIP address.
const fn uncached(addr: usize) -> usize {
    addr - XIP_BASE + XIP_NOCACHE_NOALLOC_BASE
}

// QMI registers, byte offsets from `pac::QMI::ptr()` (datasheet §12.14.6).
const QMI_DIRECT_CSR: usize = 0x00;
const QMI_DIRECT_TX: usize = 0x04;
const QMI_DIRECT_RX: usize = 0x08;
const QMI_M1_TIMING: usize = 0x20;
const QMI_M1_RFMT: usize = 0x24;
const QMI_M1_RCMD: usize = 0x28;
const QMI_M1_WFMT: usize = 0x2c;
const QMI_M1_WCMD: usize = 0x30;
/// ATRANS0..7: eight 4 MB translation windows, 4..7 for chip select 1.
const QMI_ATRANS0: usize = 0x34;
/// XIP_CTRL.CTRL, byte offset from `pac::XIP_CTRL::ptr()`.
const XIP_CTRL_CTRL: usize = 0x00;
const XIP_CTRL_WRITABLE_M1: u32 = 1 << 11;

// DIRECT_CSR fields.
const CSR_EN: u32 = 1 << 0;
const CSR_BUSY: u32 = 1 << 1;
const CSR_ASSERT_CS1N: u32 = 1 << 3;
const CSR_AUTO_CS1N: u32 = 1 << 7;
const CSR_TXEMPTY: u32 = 1 << 11;
const CSR_CLKDIV_LSB: u32 = 22;
// DIRECT_TX fields.
const TX_IWIDTH_LSB: u32 = 16;
const TX_OE: u32 = 1 << 19;
const TX_NOPUSH: u32 = 1 << 20;
/// Transfer widths, shared by DIRECT_TX.IWIDTH and the M1_*FMT phases.
const WIDTH_Q: u32 = 2;
// M1_TIMING fields.
const TIMING_RXDELAY_LSB: u32 = 8;
const TIMING_MIN_DESELECT_LSB: u32 = 12;
const TIMING_MAX_SELECT_LSB: u32 = 17;
const TIMING_PAGEBREAK_LSB: u32 = 28;
const TIMING_COOLDOWN_LSB: u32 = 30;
const PAGEBREAK_1024: u32 = 2;
// M1_RFMT / M1_WFMT fields.
const FMT_ADDR_WIDTH_LSB: u32 = 2;
const FMT_SUFFIX_WIDTH_LSB: u32 = 4;
const FMT_DUMMY_WIDTH_LSB: u32 = 6;
const FMT_DATA_WIDTH_LSB: u32 = 8;
const FMT_PREFIX_LEN_LSB: u32 = 12;
const FMT_DUMMY_LEN_LSB: u32 = 16;
const PREFIX_LEN_8: u32 = 1;
const DUMMY_LEN_24: u32 = 6;

// The APS6404's command set (its datasheet §6).
const CMD_READ_ID: u32 = 0x9F;
const CMD_NOP: u32 = 0xFF;
const CMD_QUAD_ENABLE: u32 = 0x35;
const CMD_QUAD_EXIT: u32 = 0xF5;
const CMD_QUAD_READ: u32 = 0xEB;
const CMD_QUAD_WRITE: u32 = 0x38;
/// The known-good-die byte every APS6404 (and the ISSI parts) answers.
const KGD_GOOD: u8 = 0x5D;

/// XIP_CS1n is pad function 9 on the pads that carry it (GPCK elsewhere).
const FUNCSEL_XIP_CS1: u8 = 9;

/// Window-1 timing for the part at `clock_hz`, pico-sdk's derivation
/// (`psram_configure_params`).
struct Timing {
    clkdiv: u64,
    rxdelay: u64,
    max_select: u64,
    min_deselect: u64,
}

const fn timing(clock_hz: u64) -> Timing {
    /// The part's rated clock.
    const MAX_PSRAM_HZ: u64 = 133_000_000;
    /// tCEM: the longest CE# may stay low. The part refreshes itself only
    /// while deselected, so the QMI breaks longer bursts at this limit; in
    /// units of 64 system clocks.
    const T_CEM_NS: u64 = 8_000;
    /// tCPH: the shortest CE# high between two bursts, in system clocks.
    const T_CPH_NS: u64 = 18;
    // A divisor of 1 needs an rxdelay of 1, which is too early above
    // 100 MHz; and a divided clock above 100 MHz wants one more.
    let mut clkdiv = clock_hz.div_ceil(MAX_PSRAM_HZ);
    if clkdiv == 1 && clock_hz > 100_000_000 {
        clkdiv = 2;
    }
    let mut rxdelay = clkdiv;
    if clock_hz / clkdiv > 100_000_000 {
        rxdelay += 1;
    }
    let period_fs = 1_000_000_000_000_000 / clock_hz;
    let max_select = (T_CEM_NS * 1_000_000) / (64 * period_fs);
    let min_deselect = (T_CPH_NS * 1_000_000).div_ceil(period_fs) - clkdiv.div_ceil(2);
    Timing {
        clkdiv,
        rxdelay,
        max_select,
        min_deselect,
    }
}

const TIMING: Timing = timing(generated::PSRAM_CLOCK_HZ);
const _: () = assert!(
    TIMING.clkdiv <= 0xff
        && TIMING.rxdelay <= 0x7
        && TIMING.min_deselect <= 0x1f
        && TIMING.max_select <= 0x3f,
    "psram: QMI window timing out of range for this clock"
);
/// The bus clock the part actually sees.
pub const BUS_HZ: u64 = generated::PSRAM_CLOCK_HZ / TIMING.clkdiv;

/// M1_TIMING: 1024-byte page breaks (a burst crossing one would need a slower
/// clock), one cooldown clock, and the derived select/deselect limits.
const M1_TIMING: u32 = (1 << TIMING_COOLDOWN_LSB)
    | (PAGEBREAK_1024 << TIMING_PAGEBREAK_LSB)
    | ((TIMING.max_select as u32) << TIMING_MAX_SELECT_LSB)
    | ((TIMING.min_deselect as u32) << TIMING_MIN_DESELECT_LSB)
    | ((TIMING.rxdelay as u32) << TIMING_RXDELAY_LSB)
    | (TIMING.clkdiv as u32); // CLKDIV, bits 7:0
/// Every transfer phase quad — the prefix width is bits 1:0 — with an 8-bit
/// command prefix and no suffix; the read adds its dummy phase below.
const FMT_ALL_QUAD_8BIT_PREFIX: u32 = WIDTH_Q
    | (WIDTH_Q << FMT_ADDR_WIDTH_LSB)
    | (WIDTH_Q << FMT_SUFFIX_WIDTH_LSB)
    | (WIDTH_Q << FMT_DUMMY_WIDTH_LSB)
    | (WIDTH_Q << FMT_DATA_WIDTH_LSB)
    | (PREFIX_LEN_8 << FMT_PREFIX_LEN_LSB);
/// Reads: an EBh prefix, then 24 dummy bits (six quad clocks of wait).
const M1_RFMT: u32 = FMT_ALL_QUAD_8BIT_PREFIX | (DUMMY_LEN_24 << FMT_DUMMY_LEN_LSB);
const M1_RCMD: u32 = CMD_QUAD_READ;
/// Writes: a 38h prefix, no dummy.
const M1_WFMT: u32 = FMT_ALL_QUAD_8BIT_PREFIX;
const M1_WCMD: u32 = CMD_QUAD_WRITE;

/// A register, by byte offset from a peripheral's base. A macro rather than
/// a helper so the RAM-resident code below never calls into `.text`.
macro_rules! reg {
    ($base:expr, $off:expr) => {
        (($base as usize) + $off) as *mut u32
    };
}

/// Bring the part up. Returns the ID bytes it answered: the known-good-die
/// marker and the EID, whose top three bits carry the density.
///
/// Runs from RAM: while the QMI is in direct mode it is not serving XIP, so
/// nothing here may fetch from flash — no calls, no `defmt`, no iterators;
/// the rules `with_xip_disabled!` follows in flash.rs. Interrupts are not
/// masked because nothing runs yet: this is pre-scheduler, single-core boot.
#[link_section = ".data"]
#[inline(never)]
unsafe fn bring_up(qmi: *mut u32, xip_ctrl: *mut u32) -> (u8, u8) {
    let csr = reg!(qmi, QMI_DIRECT_CSR);
    let tx = reg!(qmi, QMI_DIRECT_TX);
    let rx = reg!(qmi, QMI_DIRECT_RX);

    // Direct mode at a conservative 5 MHz for the ID exchange, chip select
    // driven by hand.
    csr.write_volatile((30 << CSR_CLKDIV_LSB) | CSR_EN);
    while csr.read_volatile() & CSR_BUSY != 0 {}

    // A warm reset — the watchdog after an install — leaves the part in QPI
    // mode, where a serial ID read gets garbage. Send exit-QPI first, as one
    // quad-width byte; in SPI mode the part ignores it.
    csr.write_volatile(csr.read_volatile() | CSR_ASSERT_CS1N);
    tx.write_volatile(TX_OE | (WIDTH_Q << TX_IWIDTH_LSB) | CMD_QUAD_EXIT);
    while csr.read_volatile() & CSR_BUSY != 0 {}
    let _ = rx.read_volatile();
    csr.write_volatile(csr.read_volatile() & !CSR_ASSERT_CS1N);

    // Read ID: the command, then seven clocked-out bytes. The sixth is the
    // known-good-die marker and the seventh the EID.
    csr.write_volatile(csr.read_volatile() | CSR_ASSERT_CS1N);
    let mut kgd = 0u8;
    let mut eid = 0u8;
    let mut i = 0u32;
    while i < 8 {
        tx.write_volatile(if i == 0 { CMD_READ_ID } else { CMD_NOP });
        while csr.read_volatile() & CSR_TXEMPTY == 0 {}
        while csr.read_volatile() & CSR_BUSY != 0 {}
        let byte = rx.read_volatile() as u8;
        if i == 5 {
            kgd = byte;
        } else if i == 6 {
            eid = byte;
        }
        i += 1;
    }
    csr.write_volatile(csr.read_volatile() & !CSR_ASSERT_CS1N);
    csr.write_volatile(0);
    if kgd != KGD_GOOD {
        return (kgd, eid);
    }

    // Into QPI mode: one serial byte with the QMI driving CS1n itself.
    // NOPUSH keeps the reply out of the RX FIFO.
    csr.write_volatile((10 << CSR_CLKDIV_LSB) | CSR_EN | CSR_AUTO_CS1N);
    while csr.read_volatile() & CSR_BUSY != 0 {}
    tx.write_volatile(TX_NOPUSH | CMD_QUAD_ENABLE);
    while csr.read_volatile() & CSR_BUSY != 0 {}
    csr.write_volatile(0);

    // Window 1.
    reg!(qmi, QMI_M1_TIMING).write_volatile(M1_TIMING);
    reg!(qmi, QMI_M1_RFMT).write_volatile(M1_RFMT);
    reg!(qmi, QMI_M1_RCMD).write_volatile(M1_RCMD);
    reg!(qmi, QMI_M1_WFMT).write_volatile(M1_WFMT);
    reg!(qmi, QMI_M1_WCMD).write_volatile(M1_WCMD);

    // XIP windows are read-only until told otherwise.
    let ctrl = reg!(xip_ctrl, XIP_CTRL_CTRL);
    ctrl.write_volatile(ctrl.read_volatile() | XIP_CTRL_WRITABLE_M1);
    (kgd, eid)
}

/// The density the EID encodes, in bytes (pico-sdk's `psram_eid_to_size`:
/// APS6404 and the ISSI parts).
fn density(eid: u8) -> usize {
    let mib = 1024 * 1024;
    match (eid, eid >> 5) {
        (_, 4) => 16 * mib,
        (0x26, _) | (_, 2) | (_, 3) => 8 * mib,
        (_, 1) => 4 * mib,
        (_, 0) => 2 * mib,
        _ => mib,
    }
}

/// The pattern the checks write: address-derived, salted per pass, so a
/// wrapped translation or a stuck data line reads back wrong.
fn pattern(addr: usize, salt: u32) -> u32 {
    (addr as u32).wrapping_mul(0x9E37_79B9) ^ salt
}

/// Three 4 KB windows — the first, the middle and the last — written through
/// the uncached alias and read back through both, so a wrong density, a dead
/// chip select or a translation that wraps show up here rather than as a
/// corrupted LVGL pool. Cache-bypassing on purpose: a check that fits in the
/// 16 KB XIP cache proves only that the cache works.
fn spot_check() -> Result<(), (usize, u32, u32)> {
    const WORDS: usize = 4096 / 4;
    let sites = [0usize, PSRAM_LEN / 2, PSRAM_LEN - 4096];
    for (pass, &site) in sites.iter().enumerate() {
        let salt = 0x5555_5555u32.wrapping_mul(pass as u32 + 1);
        let cached = (PSRAM_ORIGIN + site) as *mut u32;
        let raw = uncached(PSRAM_ORIGIN + site) as *mut u32;
        for w in 0..WORDS {
            // SAFETY: inside the window the descriptor declares and the part
            // has just been sized against; nothing else uses it yet.
            unsafe { raw.add(w).write_volatile(pattern(site + w * 4, salt)) };
        }
        for w in 0..WORDS {
            let want = pattern(site + w * 4, salt);
            for p in [raw, cached] {
                let got = unsafe { p.add(w).read_volatile() };
                if got != want {
                    return Err((site + w * 4, want, got));
                }
            }
        }
    }
    Ok(())
}

/// Bring the PSRAM up and prove it. Pre-scheduler, before anything can want
/// the window: the LVGL pool is created there on a board with
/// `lv_mem_in_psram`, and the first runtime flash write has to find window 1
/// programmed so it can save and restore it.
///
/// A part that does not answer, or answers smaller than the descriptor
/// claims, is an error on every board and fatal on one whose LVGL pool
/// lives there — an unbacked pool corrupts itself silently, and a clear line
/// at boot beats that.
pub fn init() {
    let p = unsafe { pac::Peripherals::steal() };
    super::gpio::ensure_io_unreset(&p);
    let cs = generated::PSRAM_CS_PIN as usize;
    p.IO_BANK0
        .gpio(cs)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(FUNCSEL_XIP_CS1) });
    // The ROM only lifts the pad isolation on GP0 for a chip select it does
    // not know about (erratum RP2350-E14); do it for the real pad.
    p.PADS_BANK0.gpio(cs).write(|w| {
        let w = w.iso().clear_bit();
        w.ie().set_bit().od().clear_bit()
    });

    let (kgd, eid) = unsafe {
        bring_up(
            pac::QMI::ptr() as *mut u32,
            pac::XIP_CTRL::ptr() as *mut u32,
        )
    };
    if kgd != KGD_GOOD {
        fail(
            defmt::intern!("psram: nothing answered the ID read on the chip select"),
            kgd as u32,
            eid as u32,
        );
        return;
    }
    let bytes = density(eid);
    if bytes < PSRAM_LEN {
        fail(
            defmt::intern!("psram: the part is smaller than the descriptor's psram_kb"),
            (bytes / 1024) as u32,
            (PSRAM_LEN / 1024) as u32,
        );
        return;
    }
    trim_translation(bytes);
    if let Err((addr, want, got)) = spot_check() {
        defmt::error!(
            "psram: spot check failed at {=usize:#x}: wrote {=u32:#010x}, read {=u32:#010x}",
            PSRAM_ORIGIN + addr,
            want,
            got
        );
        fatal_if_needed();
        return;
    }
    defmt::info!(
        "psram: {=usize} KB at {=usize:#x}, bus {=u64} MHz (QMI clkdiv {=u64}, rxdelay {=u64})",
        PSRAM_LEN / 1024,
        PSRAM_ORIGIN,
        BUS_HZ / 1_000_000,
        TIMING.clkdiv,
        TIMING.rxdelay
    );
    #[cfg(feature = "psram-sweep")]
    sweep();
}

fn fail(what: defmt::Str, a: u32, b: u32) {
    defmt::error!(
        "{=istr} ({=u32:#x}, {=u32:#x}); the window at {=usize:#x} is unbacked",
        what,
        a,
        b,
        PSRAM_ORIGIN
    );
    fatal_if_needed();
}

/// With the LVGL pool configured into the window, a missing part is fatal.
fn fatal_if_needed() {
    #[cfg(lv_mem_in_psram)]
    defmt::panic!(
        "psram: this board's LVGL pool lives in PSRAM (lv_mem_in_psram); cannot boot without it"
    );
}

/// Un-map the 4 MB translation windows past the end of the part, so an
/// address beyond it faults instead of wrapping onto the start.
fn trim_translation(bytes: usize) {
    let backed = bytes.min(PSRAM_LEN);
    for i in 4..8usize {
        let start = (i - 4) * 4 * 1024 * 1024;
        if start >= backed {
            // SAFETY: the QMI is idle between XIP bursts and the register is
            // ours to write; the windows cleared are ones nothing maps.
            unsafe { reg!(pac::QMI::ptr(), QMI_ATRANS0 + i * 4).write_volatile(0) };
        }
    }
}

/// Write every dirty XIP cache line back.
///
/// Clean by set/way, addressed through the top 16 KB of the maintenance
/// alias: erratum RP2350-E11 makes a clean also rewrite the line's tag, and
/// up there the tag it lands on maps nothing (pico-sdk's
/// `xip_cache_clean_all` does the same). A macro because it also runs inside
/// the XIP-off window's RAM-resident code, where nothing may call into
/// `.text`.
macro_rules! xip_cache_clean_all {
    () => {{
        const OP_CLEAN_BY_SET_WAY: usize = 1;
        let mut addr = $crate::hal::psram::XIP_CACHE_CLEAN_BASE + OP_CLEAN_BY_SET_WAY;
        let end = addr + $crate::hal::psram::XIP_CACHE_SIZE;
        while addr < end {
            (addr as *mut u8).write_volatile(0);
            addr = addr.wrapping_add(8);
        }
        core::arch::asm!("dsb sy", "isb sy", options(nostack, preserves_flags));
    }};
}
pub(crate) use xip_cache_clean_all;
/// Where the clean-all writes: the maintenance alias of the last 16 KB of
/// the XIP space.
pub const XIP_CACHE_CLEAN_BASE: usize =
    XIP_MAINTENANCE_BASE + (XIP_END - XIP_BASE) - XIP_CACHE_SIZE;

/// The whole part, twice, with inverted patterns: written through the cache,
/// cleaned, read back uncached and then cached. Reports the three bandwidths
/// as well — the number docs/designs/psram-lvgl-fluid-scroll-2026-09.md §5
/// cannot be judged without.
#[cfg(feature = "psram-sweep")]
fn sweep() {
    use super::system_clock::elapsed_realtime_nanos as now;
    let words = PSRAM_LEN / 4;
    let cached = PSRAM_ORIGIN as *mut u32;
    let raw = uncached(PSRAM_ORIGIN) as *mut u32;
    let kb_per_s = |bytes: usize, ns: i64| -> u32 {
        if ns <= 0 {
            return 0;
        }
        ((bytes as u64 * 1_000_000_000) / (ns as u64 * 1024)) as u32
    };
    for (pass, salt) in [0u32, 0xFFFF_FFFF].into_iter().enumerate() {
        let t0 = now();
        for w in 0..words {
            unsafe { cached.add(w).write_volatile(pattern(w * 4, salt)) };
        }
        unsafe { xip_cache_clean_all!() };
        let t1 = now();
        let mut bad = 0u32;
        let mut first: Option<(usize, u32, u32)> = None;
        for w in 0..words {
            let want = pattern(w * 4, salt);
            let got = unsafe { raw.add(w).read_volatile() };
            if got != want {
                bad += 1;
                first.get_or_insert((w * 4, want, got));
            }
        }
        let t2 = now();
        for w in 0..words {
            let want = pattern(w * 4, salt);
            let got = unsafe { cached.add(w).read_volatile() };
            if got != want {
                bad += 1;
                first.get_or_insert((w * 4, want, got));
            }
        }
        let t3 = now();
        defmt::info!(
            "psram sweep {=usize}: {=usize} KB, {=u32} bad words; write {=u32} KB/s (cached), read {=u32} KB/s (uncached) / {=u32} KB/s (cached)",
            pass,
            PSRAM_LEN / 1024,
            bad,
            kb_per_s(PSRAM_LEN, t1 - t0),
            kb_per_s(PSRAM_LEN, t2 - t1),
            kb_per_s(PSRAM_LEN, t3 - t2)
        );
        if let Some((addr, want, got)) = first {
            defmt::error!(
                "psram sweep {=usize}: first bad word at {=usize:#x}: wrote {=u32:#010x}, read {=u32:#010x}",
                pass,
                PSRAM_ORIGIN + addr,
                want,
                got
            );
        }
    }
}
