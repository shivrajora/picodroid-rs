// SPDX-License-Identifier: GPL-3.0-only
#[cfg(feature = "chip-rp2040")]
use rp_pico::hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

#[cfg(feature = "chip-rp2350")]
use rp235x_hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

/// RP2350 bootrom block loop: IMAGE_DEF + END block.
///
/// The bootrom requires a circular linked list of at least two blocks.
/// Both are placed right after .vector_table (which lives at flash origin
/// 0x10000000).  Each block is 20 bytes (5 words); the offset field is a
/// signed byte offset from this block's start marker to the next block's
/// start marker.
#[cfg(feature = "chip-rp2350")]
#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: [u32; 10] = [
    // Block 1: IMAGE_DEF (secure ARM executable for RP2350)
    0xffff_ded3, // BLOCK_MARKER_START
    0x1021_0142, // IMAGE_TYPE: EXE | CHIP_RP2350 | CPU_ARM | SECURITY_S
    0x0000_01ff, // ITEM_LAST(1)
    0x0000_0014, // offset: +20 bytes → end block
    0xab12_3579, // BLOCK_MARKER_END
    // Block 2: END block (closes the loop)
    0xffff_ded3, // BLOCK_MARKER_START
    0x0000_01fe, // ITEM_2BS_IGNORED (placeholder)
    0x0000_01ff, // ITEM_LAST(1)
    0xffff_ffec, // offset: −20 bytes → IMAGE_DEF block
    0xab12_3579, // BLOCK_MARKER_END
];

#[cfg(feature = "chip-rp2040")]
pub fn clock_init() {
    // RP2040: 12 MHz crystal → 125 MHz system clock
    let mut pac = pac::Peripherals::take().unwrap();
    let _sio = Sio::new(pac.SIO);
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let _clocks = init_clocks_and_plls(
        12_000_000u32,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();
}

#[cfg(feature = "chip-rp2350")]
pub fn clock_init() {
    // RP2350: 12 MHz crystal → 150 MHz system clock
    let mut pac = pac::Peripherals::take().unwrap();
    let _sio = Sio::new(pac.SIO);
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let _clocks = init_clocks_and_plls(
        12_000_000u32,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();
}
