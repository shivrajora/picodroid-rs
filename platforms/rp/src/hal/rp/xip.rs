// SPDX-License-Identifier: GPL-3.0-only
//! RP2350 XIP window 0: the flash clock the boot ROM left, made faster.
//!
//! The image carries no boot stage 2, so window 0 runs in the mode the ROM
//! discovered: quad-I/O `EB` reads with an 8-bit command prefix per burst,
//! at the ROM's divider of 3 (50 MHz SCK once `clock_init` has clk_sys at
//! 150 MHz). The JVM and LVGL working set is far larger than the 16 KB XIP
//! cache, so on UI code nearly every instruction fetch and every class-file
//! read is a miss, and each miss pays that rate: a claudeusage page-turn
//! step measured 20–25 % faster at divider 2 (D4 in
//! docs/designs/claudeusage-gaps-roadmap-2026-09.md, 2026-09-25).
//!
//! Divider 2 and RXDELAY 2 are what the pico-sdk's own boot2 programs for
//! every RP2350 board (`boot2_w25q080.S`: `PICO_FLASH_SPI_CLKDIV 2`,
//! `PICO_FLASH_SPI_RXDELAY 2`); the quad-I/O read is rated to at least
//! 104 MHz on every part the Pico 2 family ships with. The retiming is
//! applied only when the ROM did pick the `EB` read: the serial `03h`
//! fallback is rated to 50 MHz and stays at the ROM's divider. The read
//! format and command are left as the ROM set them, so the chip stays in
//! serial-command state and `flash.rs::with_xip_disabled!` restores this
//! timing after every runtime flash write by writing the registers back.

use super::system_clock::elapsed_realtime_nanos as now;
use rp235x_hal::pac;

const QMI_M0_TIMING: usize = 0x0c;
const XIP_NOCACHE_NOALLOC_BASE: usize = 0x1400_0000;
const TIMING_CLKDIV_MASK: u32 = 0xff;
const TIMING_RXDELAY_LSB: u32 = 8;
const TIMING_RXDELAY_MASK: u32 = 0x7 << TIMING_RXDELAY_LSB;
const RCMD_PREFIX_MASK: u32 = 0xff;
/// Quad-I/O fast read, the command the ROM's probe settles on.
const CMD_QUAD_IO_READ: u32 = 0xEB;
const FAST_CLKDIV: u32 = 2;
const FAST_RXDELAY: u32 = 2;

/// Bytes read to report the window's rate; ~4.5 ms at the fast timing.
const PROBE_BYTES: usize = 128 * 1024;

fn m0() -> *mut u32 {
    (pac::QMI::ptr() as usize + QMI_M0_TIMING) as *mut u32
}

/// (M0_TIMING, M0_RFMT, M0_RCMD).
fn regs() -> (u32, u32, u32) {
    let m0 = m0();
    // SAFETY: read-only register access.
    unsafe {
        (
            m0.read_volatile(),
            m0.add(1).read_volatile(),
            m0.add(2).read_volatile(),
        )
    }
}

/// RAM-resident, so the write cannot race an instruction fetch of its own
/// code through the window it retimes.
#[link_section = ".data"]
#[inline(never)]
unsafe fn set_m0_timing(timing: u32) {
    let m0 = m0();
    core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    m0.write_volatile(timing);
    core::arch::asm!("dsb sy", "isb sy", options(nostack, preserves_flags));
}

/// Microseconds to read `PROBE_BYTES` through the uncached alias: the raw
/// rate of the window, the figure a wrong restore after a flash write
/// shows first (the 14x of `flash.rs`).
fn time_uncached_read() -> u32 {
    let p = XIP_NOCACHE_NOALLOC_BASE as *const u32;
    let t0 = now();
    let mut acc = 0u32;
    for i in 0..PROBE_BYTES / 4 {
        // SAFETY: the program region is longer than the probe and readable.
        acc = acc.wrapping_add(unsafe { p.add(i).read_volatile() });
    }
    core::hint::black_box(acc);
    ((now() - t0) / 1000) as u32
}

/// Retime window 0. Once, after `clock_init` (the divider is relative to
/// clk_sys) and before anything else touches the QMI (`psram::init`).
pub fn init() {
    let (timing, rfmt, rcmd) = regs();
    let clkdiv = timing & TIMING_CLKDIV_MASK;
    let rxdelay = (timing & TIMING_RXDELAY_MASK) >> TIMING_RXDELAY_LSB;
    if rcmd & RCMD_PREFIX_MASK != CMD_QUAD_IO_READ {
        defmt::info!(
            "[xip] window 0 left as the ROM set it: rcmd {=u32:#x} rfmt {=u32:#x} clkdiv {=u32} rxdelay {=u32}",
            rcmd,
            rfmt,
            clkdiv,
            rxdelay
        );
        return;
    }
    let fast = (timing & !(TIMING_CLKDIV_MASK | TIMING_RXDELAY_MASK))
        | FAST_CLKDIV
        | (FAST_RXDELAY << TIMING_RXDELAY_LSB);
    // SAFETY: single-threaded boot, before the scheduler and before the
    // PSRAM bring-up; the write is made from RAM.
    unsafe { set_m0_timing(fast) };
    let us = time_uncached_read();
    defmt::info!(
        "[xip] window 0: quad EB, clkdiv {=u32} -> {=u32}, rxdelay {=u32} -> {=u32}; 128 KB uncached read {=u32} us",
        clkdiv,
        FAST_CLKDIV,
        rxdelay,
        FAST_RXDELAY,
        us
    );
}
